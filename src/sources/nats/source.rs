use std::time::Duration;

use async_nats::jetstream::{
    AckKind,
    consumer::{AckPolicy, PullConsumer},
    message::Acker,
};
use chrono::Utc;
use futures::{StreamExt, stream::FuturesUnordered};
use snafu::ResultExt;
use vector_lib::{
    EstimatedJsonEncodedSizeOf,
    codecs::{DecoderFramedRead, decoding::StreamDecodingError},
    config::{LegacyKey, LogNamespace},
    event::{BatchNotifier, BatchStatus, BatchStatusReceiver},
    internal_event::{
        ByteSize, BytesReceived, CountByteSize, EventsReceived, EventsReceivedHandle,
        InternalEventHandle as _, Protocol,
    },
    lookup::owned_value_path,
};

use crate::{
    SourceSender,
    codecs::Decoder,
    common::backoff::ExponentialBackoff,
    event::Event,
    internal_events::StreamClosedError,
    shutdown::ShutdownSignal,
    sources::nats::config::{
        BuildError, ConsumerSnafu, JetStreamConfig, MessagesSnafu, NatsSourceConfig, StreamSnafu,
        SubscribeSnafu,
    },
};

/// The outcome of processing a single NATS message.
pub enum ProcessingStatus {
    /// The message payload was fully decoded and sent downstream.
    Success(Option<BatchStatusReceiver>),
    /// A non-recoverable error occurred while decoding the payload.
    Failed,
    /// The downstream channel is closed, and the source should shut down.
    ChannelClosed,
}

/// Processes a single NATS message, sending decoded events downstream.
///
/// This function contains the common logic for both Core and JetStream NATS.
pub async fn process_message(
    msg: &async_nats::Message,
    config: &NatsSourceConfig,
    decoder: &Decoder,
    log_namespace: LogNamespace,
    out: &mut SourceSender,
    events_received: &EventsReceivedHandle,
    acknowledgements: bool,
) -> ProcessingStatus {
    let mut framed = DecoderFramedRead::new(msg.payload.as_ref(), decoder.clone());
    let mut success = true;
    let (batch, receiver) = BatchNotifier::maybe_new_with_receiver(acknowledgements);

    while let Some(next) = framed.next().await {
        match next {
            Ok((events, _byte_size)) => {
                let count = events.len();
                if count == 0 {
                    continue;
                }

                let byte_size = events.estimated_json_encoded_size_of();
                events_received.emit(CountByteSize(count, byte_size));
                let now = Utc::now();
                let events = events.into_iter().map(|mut event| {
                    if let Event::Log(ref mut log) = event {
                        log_namespace.insert_standard_vector_source_metadata(
                            log,
                            NatsSourceConfig::NAME,
                            now,
                        );
                        let legacy_subject_key_field = config
                            .subject_key_field
                            .path
                            .as_ref()
                            .map(LegacyKey::InsertIfEmpty);
                        log_namespace.insert_source_metadata(
                            NatsSourceConfig::NAME,
                            log,
                            legacy_subject_key_field,
                            &owned_value_path!("subject"),
                            msg.subject.as_str(),
                        );
                    }
                    event.with_batch_notifier_option(&batch)
                });

                if out.send_batch(events).await.is_err() {
                    emit!(StreamClosedError { count });
                    return ProcessingStatus::ChannelClosed;
                }
            }
            Err(error) => {
                success = false;
                // Error is logged by `vector_lib::codecs::Decoder`, no further
                // handling is needed here.
                if !error.can_continue() {
                    break;
                }
            }
        }
    }

    if !success {
        return ProcessingStatus::Failed;
    }

    ProcessingStatus::Success(receiver)
}

fn ack_progress_interval(ack_wait: Duration) -> Duration {
    if ack_wait.is_zero() {
        Duration::from_secs(1)
    } else {
        ack_wait
            .checked_div(2)
            .filter(|delay| !delay.is_zero())
            .unwrap_or(ack_wait)
    }
}

async fn wait_for_delivery(
    acker: &Acker,
    receiver: &mut BatchStatusReceiver,
    ack_wait: Duration,
) -> BatchStatus {
    let mut progress = tokio::time::interval(ack_progress_interval(ack_wait));

    loop {
        tokio::select! {
            status = &mut *receiver => return status,
            _ = progress.tick() => {
                if let Err(err) = acker.ack_with(AckKind::Progress).await {
                    error!(message = "Failed to extend JetStream message acknowledgement deadline.", %err);
                }
            }
        }
    }
}

async fn acknowledge(acker: &Acker) {
    if let Err(err) = acker.ack().await {
        error!(message = "Failed to acknowledge JetStream message.", %err);
    }
}

async fn finalize_message(acker: Acker, mut receiver: BatchStatusReceiver, ack_wait: Duration) {
    if wait_for_delivery(&acker, &mut receiver, ack_wait).await == BatchStatus::Delivered {
        acknowledge(&acker).await;
    }
}

fn handle_ack_task_result(result: Result<(), tokio::task::JoinError>) {
    if let Err(error) = result {
        error!(message = "JetStream acknowledgement task failed.", %error);
    }
}

async fn drain_ack_tasks(tasks: &mut FuturesUnordered<tokio::task::JoinHandle<()>>) {
    while let Some(result) = tasks.next().await {
        handle_ack_task_result(result);
    }
}

pub(crate) async fn create_consumer_stream(
    connection: &async_nats::Client,
    js_config: &JetStreamConfig,
    acknowledgements: bool,
) -> Result<(async_nats::jetstream::consumer::pull::Stream, Duration), BuildError> {
    let js = async_nats::jetstream::new(connection.clone());
    let stream = js
        .get_stream(&js_config.stream)
        .await
        .context(StreamSnafu)?;
    let consumer: PullConsumer = stream
        .get_consumer(&js_config.consumer)
        .await
        .context(ConsumerSnafu)?;
    let consumer_config = &consumer.cached_info().config;
    if acknowledgements && consumer_config.ack_policy != AckPolicy::Explicit {
        return Err(BuildError::InvalidAckPolicy {
            policy: consumer_config.ack_policy,
        });
    }
    let ack_wait = consumer_config.ack_wait;
    let messages = consumer
        .stream()
        .max_messages_per_batch(js_config.batch_config.batch)
        .max_bytes_per_batch(js_config.batch_config.max_bytes)
        .messages()
        .await
        .context(MessagesSnafu)?;
    Ok((messages, ack_wait))
}

pub async fn run_nats_jetstream(
    config: NatsSourceConfig,
    connection: async_nats::Client,
    initial_messages: async_nats::jetstream::consumer::pull::Stream,
    mut ack_wait: Duration,
    decoder: Decoder,
    log_namespace: LogNamespace,
    mut shutdown: ShutdownSignal,
    mut out: SourceSender,
    acknowledgements: bool,
) -> Result<(), ()> {
    let events_received = register!(EventsReceived);
    let bytes_received = register!(BytesReceived::from(Protocol::TCP));
    let mut backoff = ExponentialBackoff::default().max_delay(std::time::Duration::from_secs(30));

    let js_config = config
        .jetstream
        .as_ref()
        .expect("jetstream config must be present");

    let mut messages = initial_messages;
    let mut finalizers = FuturesUnordered::new();

    loop {
        // `ShutdownSignal` fires once then polls `Pending` forever, so shutdown must be handled here via `select!`, not re-polled afterwards.
        loop {
            tokio::select! {
                biased;

                _ = &mut shutdown => {
                    drop(messages);
                    drop(out);
                    drain_ack_tasks(&mut finalizers).await;
                    return Ok(());
                },

                Some(result) = finalizers.next(), if !finalizers.is_empty() => {
                    handle_ack_task_result(result);
                }

                maybe_msg = messages.next() => {
                    match maybe_msg {
                        Some(Ok(msg)) => {
                            let (msg, acker) = msg.split();
                            backoff.reset();
                            bytes_received.emit(ByteSize(msg.payload.len()));

                            let status = process_message(
                                &msg,
                                &config,
                                &decoder,
                                log_namespace,
                                &mut out,
                                &events_received,
                                acknowledgements,
                            )
                            .await;

                            match status {
                                ProcessingStatus::Success(Some(receiver)) => {
                                    finalizers.push(crate::spawn_in_current_span(
                                        finalize_message(acker, receiver, ack_wait)
                                    ));
                                }
                                ProcessingStatus::Success(None) => acknowledge(&acker).await,
                                ProcessingStatus::ChannelClosed => return Err(()),
                                // Do not acknowledge on failure; the message will be redelivered.
                                ProcessingStatus::Failed => {}
                            }
                        }
                        Some(Err(err)) => {
                            warn!(message = "JetStream consumer stream error, recreating.", %err);
                            break;
                        }
                        // The pull stream ended; recover the consumer.
                        None => break,
                    }
                }
            }
        }

        // Reconnect: rebuild the consumer stream with backoff.
        // The durable consumer on the server tracks delivery state,
        // so we pick up where we left off.
        warn!(message = "JetStream pull stream terminated. Recovering consumer...");
        // Drop the failed stream so its background pull task stops issuing pulls and
        // buffering ack-pending deliveries while we back off.
        drop(messages);
        loop {
            let delay = backoff.next().expect("backoff never ends");
            let reconnect = tokio::time::sleep(delay);
            tokio::pin!(reconnect);

            loop {
                tokio::select! {
                    _ = &mut shutdown => {
                        drop(out);
                        drain_ack_tasks(&mut finalizers).await;
                        return Ok(());
                    }

                    Some(result) = finalizers.next(), if !finalizers.is_empty() => {
                        handle_ack_task_result(result);
                    }

                    _ = &mut reconnect => break,
                }
            }

            match create_consumer_stream(&connection, js_config, acknowledgements).await {
                Ok((new_messages, new_ack_wait)) => {
                    // Don't reset backoff on construction; a built stream hasn't pulled
                    // yet. Backoff is reset only after a message is successfully pulled.
                    messages = new_messages;
                    ack_wait = new_ack_wait;
                    break;
                }
                Err(err) => {
                    warn!(message = "Failed to recreate JetStream consumer stream, retrying.", %err);
                }
            }
        }
    }
}

pub async fn run_nats_core(
    config: NatsSourceConfig,
    _connection: async_nats::Client,
    mut subscriber: async_nats::Subscriber,
    decoder: Decoder,
    log_namespace: LogNamespace,
    mut shutdown: ShutdownSignal,
    mut out: SourceSender,
) -> Result<(), ()> {
    let events_received = register!(EventsReceived);
    let bytes_received = register!(BytesReceived::from(Protocol::TCP));

    loop {
        tokio::select! {
            biased;

             _ = &mut shutdown => {
                info!("Shutdown signal received. Draining NATS subscription...");
                if let Err(err) = subscriber.drain().await {
                    error!(message = "Failed to drain NATS subscription.", %err);
                }
            },

            maybe_msg = subscriber.next() => {
                match maybe_msg {
                    Some(msg) => {
                        bytes_received.emit(ByteSize(msg.payload.len()));
                        let status = process_message(
                            &msg,
                            &config,
                            &decoder,
                            log_namespace,
                            &mut out,
                            &events_received,
                            false,
                        )
                        .await;

                        if let ProcessingStatus::ChannelClosed = status {
                            return Err(());
                        }
                    },
                    None => {
                        // The stream has ended. This happens naturally after a successful
                        // drain or if the connection is lost.
                        break;
                    }
                }
            }
        }
    }

    info!("NATS source drained and shut down gracefully.");
    Ok(())
}

pub async fn create_subscription(
    config: &NatsSourceConfig,
) -> Result<(async_nats::Client, async_nats::Subscriber), BuildError> {
    let nc = config.connect().await?;

    let subscription = match &config.queue {
        None => nc.subscribe(config.subject.clone()).await,
        Some(queue) => {
            nc.queue_subscribe(config.subject.clone(), queue.clone())
                .await
        }
    };

    let subscription = subscription.context(SubscribeSnafu)?;

    Ok((nc, subscription))
}
