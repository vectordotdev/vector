use std::{
    collections::HashMap,
    future::Future,
    panic,
    pin::Pin,
    str::FromStr,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    task::{Context, Poll},
    time::Duration,
};

use aws_sdk_sqs::{
    Client as SqsClient,
    types::{DeleteMessageBatchRequestEntry, MessageSystemAttributeName},
};
use chrono::{DateTime, TimeZone, Utc};
use futures::{FutureExt, StreamExt, stream::FuturesUnordered};
use tokio::{
    pin, select,
    sync::{Notify, watch},
};
use vector_lib::{
    config::LogNamespace,
    finalizer::UnorderedFinalizer,
    internal_event::{EventsReceived, Registered},
};

use crate::{
    SourceSender,
    codecs::Decoder,
    event::{BatchNotifier, BatchStatus},
    internal_events::{
        EndpointBytesReceived, SqsMessageDeleteError, SqsMessageReceiveError, StreamClosedError,
    },
    shutdown::ShutdownSignal,
    sources::util,
};

// This is the maximum SQS supports in a single batch request
const MAX_BATCH_SIZE: i32 = 10;

// Mark a polling worker as busy when it receives at least half of MAX_BATCH_SIZE messages.
// Standard SQS queues distribute messages across partitions and may return fewer than
// MAX_BATCH_SIZE messages even when a backlog exists.
const BUSY_BATCH_THRESHOLD: usize = (MAX_BATCH_SIZE as usize) / 2;

const RECEIVE_ERROR_RETRY_DELAY: Duration = Duration::from_secs(1);

type Finalizer = UnorderedFinalizer<Vec<String>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PollOutcome {
    Messages(usize),
    ReceiveError,
    StreamClosed,
}

#[derive(Clone)]
pub struct SqsSource {
    pub client: SqsClient,
    pub queue_url: String,
    pub decoder: Decoder,
    pub poll_secs: u32,
    pub visibility_timeout_secs: u32,
    pub delete_message: bool,
    pub max_concurrency: usize,
    pub(super) acknowledgements: bool,
    pub(super) log_namespace: LogNamespace,
}

impl SqsSource {
    pub async fn run(self, out: SourceSender, shutdown: ShutdownSignal) -> Result<(), ()> {
        let finalizer = self.acknowledgements.then(|| {
            let (finalizer, mut ack_stream) = Finalizer::new(Some(shutdown.clone()));
            let client = self.client.clone();
            let queue_url = self.queue_url.clone();
            crate::spawn_in_current_span(async move {
                while let Some((status, receipts)) = ack_stream.next().await {
                    if status == BatchStatus::Delivered {
                        delete_messages(client.clone(), receipts, queue_url.clone()).await;
                    }
                }
            });
            Arc::new(finalizer)
        });
        let events_received = register!(EventsReceived);
        let max_concurrency = self.max_concurrency;

        run_adaptive_concurrency(
            max_concurrency,
            shutdown,
            Arc::new(AtomicUsize::new(0)),
            move || {
                let source = self.clone();
                let mut out = out.clone();
                let finalizer = finalizer.clone();
                let events_received = events_received.clone();
                async move {
                    source
                        .run_once(&mut out, finalizer.as_ref(), events_received)
                        .await
                }
            },
        )
        .await
    }

    #[allow(
        clippy::cast_possible_wrap,
        reason = "Preserve the existing numeric conversion semantics of this source. https://github.com/vectordotdev/vector/issues/23659"
    )]
    async fn run_once(
        &self,
        out: &mut SourceSender,
        finalizer: Option<&Arc<Finalizer>>,
        events_received: Registered<EventsReceived>,
    ) -> PollOutcome {
        let result = self
            .client
            .receive_message()
            .queue_url(&self.queue_url)
            .max_number_of_messages(MAX_BATCH_SIZE)
            .wait_time_seconds(self.poll_secs as i32)
            .visibility_timeout(self.visibility_timeout_secs as i32)
            .message_system_attribute_names(MessageSystemAttributeName::from("SentTimestamp"))
            // I think this should be a known attribute
            // https://github.com/awslabs/aws-sdk-rust/issues/411
            .send()
            .await;

        let receive_message_output = match result {
            Ok(output) => output,
            Err(err) => {
                emit!(SqsMessageReceiveError { error: &err });
                return PollOutcome::ReceiveError;
            }
        };

        let Some(messages) = receive_message_output.messages else {
            return PollOutcome::Messages(0);
        };
        if messages.is_empty() {
            return PollOutcome::Messages(0);
        }

        let message_count = messages.len();
        let byte_size = messages
            .iter()
            .map(|message| message.body().map_or(0, str::len))
            .sum();
        emit!(EndpointBytesReceived {
            byte_size,
            protocol: "http",
            endpoint: &self.queue_url
        });

        let mut receipts_to_ack = Vec::with_capacity(messages.len());
        let mut events = Vec::with_capacity(messages.len());

        let (batch, batch_receiver) = BatchNotifier::maybe_new_with_receiver(finalizer.is_some());
        for message in messages {
            if let Some(body) = message.body {
                // a receipt handle should always exist
                if let Some(receipt_handle) = message.receipt_handle {
                    receipts_to_ack.push(receipt_handle);
                }
                let timestamp = get_timestamp(message.attributes.as_ref());
                // Error is logged by `vector_lib::codecs::Decoder`, no further handling
                // is needed here.
                let decoded = util::decode_message(
                    self.decoder.clone(),
                    "aws_sqs",
                    body.as_bytes(),
                    timestamp,
                    &batch,
                    self.log_namespace,
                    &events_received,
                );
                events.extend(decoded);
            }
        }
        drop(batch); // Drop last reference to batch acknowledgement finalizer
        let count = events.len();

        if let Ok(()) = out.send_batch(events).await {
            if self.delete_message {
                match (finalizer, batch_receiver) {
                    (Some(finalizer), Some(receiver)) => finalizer.add(receipts_to_ack, receiver),
                    _ => {
                        delete_messages(
                            self.client.clone(),
                            receipts_to_ack,
                            self.queue_url.clone(),
                        )
                        .await;
                    }
                }
            }
            PollOutcome::Messages(message_count)
        } else {
            emit!(StreamClosedError { count });
            PollOutcome::StreamClosed
        }
    }
}

#[pin_project::pin_project]
struct Task {
    #[pin]
    handle: tokio::task::JoinHandle<()>,
    busy_flag: Arc<AtomicBool>,
    active_flag: Arc<AtomicBool>,
}

impl Future for Task {
    type Output = Result<(), tokio::task::JoinError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.project().handle.poll(cx)
    }
}

fn spawn_worker_task<F, Fut>(
    tasks: &FuturesUnordered<Task>,
    concurrency: &Arc<AtomicUsize>,
    scale_up: &Arc<Notify>,
    stream_closed_tx: &Arc<watch::Sender<bool>>,
    mut stream_closed_rx: watch::Receiver<bool>,
    poll_fn: &Arc<F>,
    shutdown: ShutdownSignal,
) where
    F: Fn() -> Fut + Send + Sync + 'static,
    Fut: Future<Output = PollOutcome> + Send + 'static,
{
    let new_concurrency = concurrency.fetch_add(1, Ordering::SeqCst) + 1;
    debug!(
        message = "Starting polling task.",
        concurrency = new_concurrency
    );
    let busy_flag = Arc::new(AtomicBool::new(false));
    let active_flag = Arc::new(AtomicBool::new(true));
    let busy_flag_worker = Arc::clone(&busy_flag);
    let active_flag_worker = Arc::clone(&active_flag);
    let concurrency_worker = Arc::clone(concurrency);
    let scale_up_worker = Arc::clone(scale_up);
    let stream_closed_tx_worker = Arc::clone(stream_closed_tx);
    let poll_fn_worker = Arc::clone(poll_fn);

    let handle = crate::spawn_in_current_span(async move {
        let shutdown = shutdown.fuse();
        pin!(shutdown);
        loop {
            select! {
                biased;
                _ = &mut shutdown => break,
                _ = stream_closed_rx.changed() => {
                    busy_flag_worker.store(false, Ordering::Release);
                    active_flag_worker.store(false, Ordering::SeqCst);
                    concurrency_worker.fetch_sub(1, Ordering::SeqCst);
                    break;
                }
                outcome = poll_fn_worker() => match outcome {
                    PollOutcome::Messages(count) if count >= BUSY_BATCH_THRESHOLD => {
                        busy_flag_worker.store(true, Ordering::Release);
                        scale_up_worker.notify_one();
                    }
                    PollOutcome::Messages(count) if count > 0 => {
                        busy_flag_worker.store(false, Ordering::Release);
                    }
                    PollOutcome::Messages(_) | PollOutcome::ReceiveError => {
                        busy_flag_worker.store(false, Ordering::Release);
                        let scaled_down = concurrency_worker
                            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |c| {
                                (c > 1).then_some(c - 1)
                            })
                            .is_ok();
                        if scaled_down {
                            active_flag_worker.store(false, Ordering::SeqCst);
                            debug!(message = "Shutting down inactive polling task.");
                            break;
                        }
                        if matches!(outcome, PollOutcome::ReceiveError) {
                            select! {
                                biased;
                                _ = &mut shutdown => break,
                                _ = stream_closed_rx.changed() => {
                                    active_flag_worker.store(false, Ordering::SeqCst);
                                    concurrency_worker.fetch_sub(1, Ordering::SeqCst);
                                    break;
                                }
                                () = tokio::time::sleep(RECEIVE_ERROR_RETRY_DELAY) => {}
                            }
                        }
                    }
                    PollOutcome::StreamClosed => {
                        busy_flag_worker.store(false, Ordering::Release);
                        active_flag_worker.store(false, Ordering::SeqCst);
                        stream_closed_tx_worker.send_replace(true);
                        concurrency_worker.fetch_sub(1, Ordering::SeqCst);
                        break;
                    }
                },
            }
        }
    });

    tasks.push(Task {
        handle,
        busy_flag,
        active_flag,
    });
}

async fn run_adaptive_concurrency<F, Fut>(
    max_concurrency: usize,
    shutdown: ShutdownSignal,
    concurrency: Arc<AtomicUsize>,
    poll_fn: F,
) -> Result<(), ()>
where
    F: Fn() -> Fut + Send + Sync + 'static,
    Fut: Future<Output = PollOutcome> + Send + 'static,
{
    let max_concurrency = max_concurrency.max(1);
    let poll_fn = Arc::new(poll_fn);
    let scale_up = Arc::new(Notify::new());
    let (stream_closed_tx, stream_closed_rx) = watch::channel(false);
    let stream_closed_tx = Arc::new(stream_closed_tx);
    let mut tasks = FuturesUnordered::new();

    spawn_worker_task(
        &tasks,
        &concurrency,
        &scale_up,
        &stream_closed_tx,
        stream_closed_rx.clone(),
        &poll_fn,
        shutdown.clone(),
    );

    let shutdown_fused = shutdown.clone().fuse();
    pin!(shutdown_fused);

    loop {
        select! {
            biased;
            _ = &mut shutdown_fused => break,
            Some(res) = tasks.next() => {
                if let Err(e) = res
                    && e.is_panic()
                {
                    panic::resume_unwind(e.into_panic());
                }
                if tasks.is_empty() {
                    if *stream_closed_rx.borrow() {
                        break;
                    }
                    concurrency.store(0, Ordering::SeqCst);
                    spawn_worker_task(
                        &tasks,
                        &concurrency,
                        &scale_up,
                        &stream_closed_tx,
                        stream_closed_rx.clone(),
                        &poll_fn,
                        shutdown.clone(),
                    );
                }
            }
            () = scale_up.notified() => {
                let current = concurrency.load(Ordering::SeqCst);
                if !*stream_closed_rx.borrow()
                    && current < max_concurrency
                    && tasks
                        .iter()
                        .filter(|t| t.active_flag.load(Ordering::SeqCst))
                        .all(|t| t.busy_flag.load(Ordering::Acquire))
                {
                    spawn_worker_task(
                        &tasks,
                        &concurrency,
                        &scale_up,
                        &stream_closed_tx,
                        stream_closed_rx.clone(),
                        &poll_fn,
                        shutdown.clone(),
                    );
                }
            }
        }
    }

    // Wait for all active polling tasks to finish. If any one of them panics, resume
    // that panic here to properly shut down Vector.
    while let Some(res) = tasks.next().await {
        if let Err(e) = res
            && e.is_panic()
        {
            panic::resume_unwind(e.into_panic());
        }
    }
    Ok(())
}

fn get_timestamp(
    attributes: Option<&HashMap<MessageSystemAttributeName, String>>,
) -> Option<DateTime<Utc>> {
    attributes.and_then(|attributes| {
        let sent_time_str = attributes.get(&MessageSystemAttributeName::SentTimestamp)?;
        Utc.timestamp_millis_opt(i64::from_str(sent_time_str).ok()?)
            .single()
    })
}

async fn delete_messages(client: SqsClient, receipts: Vec<String>, queue_url: String) {
    if !receipts.is_empty() {
        let mut batch = client.delete_message_batch().queue_url(queue_url);

        for (id, receipt) in receipts.into_iter().enumerate() {
            batch = batch.entries(
                DeleteMessageBatchRequestEntry::builder()
                    .id(id.to_string())
                    .receipt_handle(receipt)
                    .build()
                    .expect("all required builder parameters specified"),
            );
        }
        if let Err(err) = batch.send().await {
            emit!(SqsMessageDeleteError { error: &err });
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        sync::{
            Arc, Mutex,
            atomic::{AtomicUsize, Ordering},
        },
        time::Duration,
    };

    use chrono::SecondsFormat;
    use tokio::sync::Notify;
    use vector_lib::lookup::path;

    use super::*;
    use crate::{
        codecs::DecodingConfig,
        config::{SourceConfig, log_schema},
        shutdown::ShutdownSignal,
        sources::aws_sqs::AwsSqsConfig,
    };

    #[tokio::test(start_paused = true)]
    async fn adaptive_concurrency_starts_with_single_worker() {
        let (trigger, shutdown, _) = ShutdownSignal::new_wired();
        let concurrency = Arc::new(AtomicUsize::new(0));
        let poll_started = Arc::new(Notify::new());
        let poll_started_clone = Arc::clone(&poll_started);

        let handle = tokio::spawn(run_adaptive_concurrency(
            8,
            shutdown,
            Arc::clone(&concurrency),
            move || {
                let poll_started = Arc::clone(&poll_started_clone);
                async move {
                    poll_started.notify_one();
                    tokio::time::sleep(Duration::from_secs(15)).await;
                    PollOutcome::Messages(0)
                }
            },
        ));

        poll_started.notified().await;
        assert_eq!(concurrency.load(Ordering::SeqCst), 1);

        drop(trigger);
        handle.await.unwrap().unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn adaptive_concurrency_scales_up_to_max_on_busy_batches() {
        let (trigger, shutdown, _) = ShutdownSignal::new_wired();
        let concurrency = Arc::new(AtomicUsize::new(0));
        let max_seen = Arc::new(AtomicUsize::new(0));
        let max_seen_clone = Arc::clone(&max_seen);
        let concurrency_clone = Arc::clone(&concurrency);
        let reached_max = Arc::new(Notify::new());
        let reached_max_clone = Arc::clone(&reached_max);

        let handle = tokio::spawn(run_adaptive_concurrency(
            4,
            shutdown,
            Arc::clone(&concurrency),
            move || {
                let max_seen = Arc::clone(&max_seen_clone);
                let concurrency = Arc::clone(&concurrency_clone);
                let reached_max = Arc::clone(&reached_max_clone);
                async move {
                    let current = concurrency.load(Ordering::SeqCst);
                    max_seen.fetch_max(current, Ordering::SeqCst);
                    if current == 4 {
                        reached_max.notify_one();
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    PollOutcome::Messages(5) // BUSY_BATCH_THRESHOLD = 5
                }
            },
        ));

        reached_max.notified().await;
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(concurrency.load(Ordering::SeqCst), 4);
        assert_eq!(max_seen.load(Ordering::SeqCst), 4);

        drop(trigger);
        handle.await.unwrap().unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn adaptive_concurrency_holds_steady_on_partial_batches() {
        let (trigger, shutdown, _) = ShutdownSignal::new_wired();
        let concurrency = Arc::new(AtomicUsize::new(0));
        let polls = Arc::new(AtomicUsize::new(0));
        let polls_clone = Arc::clone(&polls);
        let done = Arc::new(Notify::new());
        let done_clone = Arc::clone(&done);

        let handle = tokio::spawn(run_adaptive_concurrency(
            4,
            shutdown,
            Arc::clone(&concurrency),
            move || {
                let polls = Arc::clone(&polls_clone);
                let done = Arc::clone(&done_clone);
                async move {
                    let n = polls.fetch_add(1, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    if n == 0 {
                        // First poll is busy -> scales from 1 to 2
                        PollOutcome::Messages(10)
                    } else {
                        if n >= 10 {
                            done.notify_one();
                        }
                        // Subsequent polls return 4 (< BUSY_BATCH_THRESHOLD) -> stays at 2
                        PollOutcome::Messages(4)
                    }
                }
            },
        ));

        done.notified().await;
        assert_eq!(concurrency.load(Ordering::SeqCst), 2);

        drop(trigger);
        handle.await.unwrap().unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn adaptive_concurrency_scales_down_to_one_on_empty_or_error() {
        let (trigger, shutdown, _) = ShutdownSignal::new_wired();
        let concurrency = Arc::new(AtomicUsize::new(0));
        let phase = Arc::new(AtomicUsize::new(0));
        let phase_clone = Arc::clone(&phase);
        let concurrency_clone = Arc::clone(&concurrency);
        let scaled_up = Arc::new(Notify::new());
        let scaled_up_clone = Arc::clone(&scaled_up);

        let handle = tokio::spawn(run_adaptive_concurrency(
            3,
            shutdown,
            Arc::clone(&concurrency),
            move || {
                let phase = Arc::clone(&phase_clone);
                let concurrency = Arc::clone(&concurrency_clone);
                let scaled_up = Arc::clone(&scaled_up_clone);
                async move {
                    if concurrency.load(Ordering::SeqCst) == 3 {
                        phase.store(1, Ordering::SeqCst);
                        scaled_up.notify_one();
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    if phase.load(Ordering::SeqCst) == 0 {
                        PollOutcome::Messages(10)
                    } else {
                        PollOutcome::Messages(0)
                    }
                }
            },
        ));

        scaled_up.notified().await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(concurrency.load(Ordering::SeqCst), 1);

        drop(trigger);
        handle.await.unwrap().unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn adaptive_concurrency_scales_up_after_scale_down() {
        let (trigger, shutdown, _) = ShutdownSignal::new_wired();
        let concurrency = Arc::new(AtomicUsize::new(0));
        let outcomes = Arc::new(Mutex::new(VecDeque::from([
            PollOutcome::Messages(10),
            PollOutcome::Messages(0),
            PollOutcome::Messages(10),
            PollOutcome::Messages(10),
        ])));
        let outcomes_clone = Arc::clone(&outcomes);

        let handle = tokio::spawn(run_adaptive_concurrency(
            2,
            shutdown,
            Arc::clone(&concurrency),
            move || {
                let outcomes = Arc::clone(&outcomes_clone);
                async move {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    outcomes
                        .lock()
                        .unwrap()
                        .pop_front()
                        .unwrap_or(PollOutcome::Messages(10))
                }
            },
        ));

        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(concurrency.load(Ordering::SeqCst), 2);

        drop(trigger);
        handle.await.unwrap().unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn adaptive_concurrency_exits_all_workers_on_stream_closed() {
        let (_trigger, shutdown, _) = ShutdownSignal::new_wired();
        let concurrency = Arc::new(AtomicUsize::new(0));

        let result = run_adaptive_concurrency(4, shutdown, Arc::clone(&concurrency), || async {
            PollOutcome::StreamClosed
        })
        .await;

        assert!(result.is_ok());
        assert_eq!(concurrency.load(Ordering::SeqCst), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn adaptive_concurrency_respects_max_concurrency_one() {
        let (trigger, shutdown, _) = ShutdownSignal::new_wired();
        let concurrency = Arc::new(AtomicUsize::new(0));
        let max_seen = Arc::new(AtomicUsize::new(0));
        let max_seen_clone = Arc::clone(&max_seen);
        let concurrency_clone = Arc::clone(&concurrency);
        let polls = Arc::new(AtomicUsize::new(0));
        let polls_clone = Arc::clone(&polls);

        let handle = tokio::spawn(run_adaptive_concurrency(
            1,
            shutdown,
            Arc::clone(&concurrency),
            move || {
                let max_seen = Arc::clone(&max_seen_clone);
                let concurrency = Arc::clone(&concurrency_clone);
                let polls = Arc::clone(&polls_clone);
                async move {
                    max_seen.fetch_max(concurrency.load(Ordering::SeqCst), Ordering::SeqCst);
                    let n = polls.fetch_add(1, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    if n < 3 {
                        PollOutcome::Messages(10)
                    } else {
                        PollOutcome::Messages(0)
                    }
                }
            },
        ));

        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(max_seen.load(Ordering::SeqCst), 1);
        assert_eq!(concurrency.load(Ordering::SeqCst), 1);

        drop(trigger);
        handle.await.unwrap().unwrap();
    }

    #[tokio::test(start_paused = true)]
    #[should_panic(expected = "simulated worker panic")]
    async fn adaptive_concurrency_propagates_worker_panic() {
        let (_trigger, shutdown, _) = ShutdownSignal::new_wired();
        let concurrency = Arc::new(AtomicUsize::new(0));

        run_adaptive_concurrency(2, shutdown, concurrency, || async {
            panic!("simulated worker panic");
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn test_decode_vector_namespace() {
        let config = AwsSqsConfig {
            log_namespace: Some(true),
            ..Default::default()
        };
        let definitions = config
            .outputs(LogNamespace::Vector)
            .remove(0)
            .schema_definition(true);

        let message = "test";
        let now = Utc::now();
        let events: Vec<_> = util::decode_message(
            DecodingConfig::new(
                config.framing.clone(),
                config.decoding,
                LogNamespace::Vector,
            )
            .build()
            .unwrap(),
            "aws_sqs",
            b"test",
            Some(now),
            &None,
            LogNamespace::Vector,
            &register!(EventsReceived),
        )
        .collect();
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0]
                .clone()
                .as_log()
                .get(vrl::event_path!())
                .unwrap()
                .to_string_lossy(),
            message
        );
        assert_eq!(
            events[0]
                .clone()
                .as_log()
                .metadata()
                .value()
                .get(path!(AwsSqsConfig::NAME, "timestamp"))
                .unwrap()
                .to_string_lossy(),
            now.to_rfc3339_opts(SecondsFormat::AutoSi, true)
        );
        definitions.unwrap().assert_valid_for_event(&events[0]);
    }

    #[tokio::test]
    async fn test_decode_legacy_namespace() {
        let config = AwsSqsConfig {
            log_namespace: None,
            ..Default::default()
        };
        let definitions = config
            .outputs(LogNamespace::Legacy)
            .remove(0)
            .schema_definition(true);

        let message = "test";
        let now = Utc::now();
        let events: Vec<_> = util::decode_message(
            DecodingConfig::new(
                config.framing.clone(),
                config.decoding,
                LogNamespace::Legacy,
            )
            .build()
            .unwrap(),
            "aws_sqs",
            b"test",
            Some(now),
            &None,
            LogNamespace::Legacy,
            &register!(EventsReceived),
        )
        .collect();
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0]
                .clone()
                .as_log()
                .get(log_schema().message_key_target_path().unwrap())
                .unwrap()
                .to_string_lossy(),
            message
        );
        assert_eq!(
            events[0]
                .clone()
                .as_log()
                .get_timestamp()
                .unwrap()
                .to_string_lossy(),
            now.to_rfc3339_opts(SecondsFormat::AutoSi, true)
        );
        definitions.unwrap().assert_valid_for_event(&events[0]);
    }

    #[test]
    fn test_get_timestamp() {
        let attributes = HashMap::from([(
            MessageSystemAttributeName::SentTimestamp,
            "1636408546018".to_string(),
        )]);

        assert_eq!(
            get_timestamp(Some(&attributes)),
            Some(
                Utc.timestamp_millis_opt(1_636_408_546_018)
                    .single()
                    .expect("invalid timestamp")
            )
        );
    }

    #[test]
    fn test_get_timestamp_ignores_invalid_and_out_of_range_values() {
        let out_of_range = HashMap::from([(
            MessageSystemAttributeName::SentTimestamp,
            i64::MAX.to_string(),
        )]);
        assert_eq!(get_timestamp(Some(&out_of_range)), None);

        let non_numeric = HashMap::from([(
            MessageSystemAttributeName::SentTimestamp,
            "not-a-timestamp".to_string(),
        )]);
        assert_eq!(get_timestamp(Some(&non_numeric)), None);
    }

    #[tokio::test(start_paused = true)]
    async fn adaptive_concurrency_scales_down_and_backs_off_on_receive_error() {
        let (trigger, shutdown, _) = ShutdownSignal::new_wired();
        let concurrency = Arc::new(AtomicUsize::new(0));
        let phase = Arc::new(AtomicUsize::new(0));
        let phase_clone = Arc::clone(&phase);
        let concurrency_clone = Arc::clone(&concurrency);
        let scaled_up = Arc::new(Notify::new());
        let scaled_up_clone = Arc::clone(&scaled_up);
        let error_polls = Arc::new(AtomicUsize::new(0));
        let error_polls_clone = Arc::clone(&error_polls);

        let handle = tokio::spawn(run_adaptive_concurrency(
            3,
            shutdown,
            Arc::clone(&concurrency),
            move || {
                let phase = Arc::clone(&phase_clone);
                let concurrency = Arc::clone(&concurrency_clone);
                let scaled_up = Arc::clone(&scaled_up_clone);
                let error_polls = Arc::clone(&error_polls_clone);
                async move {
                    if concurrency.load(Ordering::SeqCst) == 3
                        && phase.swap(1, Ordering::SeqCst) == 0
                    {
                        scaled_up.notify_one();
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    if phase.load(Ordering::SeqCst) == 0 {
                        PollOutcome::Messages(10)
                    } else {
                        error_polls.fetch_add(1, Ordering::SeqCst);
                        PollOutcome::ReceiveError
                    }
                }
            },
        ));

        scaled_up.notified().await;
        // Within 100ms, surplus workers scale down to 1 and the floor worker enters 1s backoff.
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(concurrency.load(Ordering::SeqCst), 1);
        let polls_before_backoff_elapsed = error_polls.load(Ordering::SeqCst);
        assert!(
            polls_before_backoff_elapsed <= 4,
            "floor worker must back off instead of hot-looping on ReceiveError, got {polls_before_backoff_elapsed}"
        );

        drop(trigger);
        handle.await.unwrap().unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn adaptive_concurrency_cancels_in_flight_siblings_on_stream_closed() {
        let (_trigger, shutdown, _) = ShutdownSignal::new_wired();
        let concurrency = Arc::new(AtomicUsize::new(0));
        let call_count = Arc::new(AtomicUsize::new(0));
        let call_count_clone = Arc::clone(&call_count);

        let handle = tokio::spawn(run_adaptive_concurrency(
            2,
            shutdown,
            Arc::clone(&concurrency),
            move || {
                let call_count = Arc::clone(&call_count_clone);
                async move {
                    match call_count.fetch_add(1, Ordering::SeqCst) {
                        0 => {
                            tokio::time::sleep(Duration::from_millis(10)).await;
                            PollOutcome::Messages(10)
                        }
                        1 => {
                            // Worker 1 blocks in a 15-second long poll
                            tokio::time::sleep(Duration::from_secs(15)).await;
                            PollOutcome::Messages(0)
                        }
                        _ => {
                            // Worker 2 encounters downstream channel closure after 10ms
                            tokio::time::sleep(Duration::from_millis(10)).await;
                            PollOutcome::StreamClosed
                        }
                    }
                }
            },
        ));

        // Advance only 50ms (< 15s long poll); supervisor and both workers must exit immediately.
        tokio::time::timeout(Duration::from_millis(50), handle)
            .await
            .expect("in-flight sibling worker should be cancelled immediately on StreamClosed")
            .unwrap()
            .unwrap();
        assert_eq!(concurrency.load(Ordering::SeqCst), 0);
    }
}
