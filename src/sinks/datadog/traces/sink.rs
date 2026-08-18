use std::{fmt::Debug, future::Future, sync::Arc, time::Duration};

use async_trait::async_trait;
use futures_util::{
    StreamExt,
    future::BoxFuture,
    stream::{self, BoxStream},
};
use tokio::sync::oneshot::{Sender, channel};
use tower::Service;
use vector_lib::{
    config::log_schema,
    event::Event,
    partition::Partitioner,
    sink::StreamSink,
    stream::{BatcherSettings, DriverResponse},
};
use vrl::{event_path, path::PathPrefix};

use super::service::TraceApiRequest;
use crate::{
    internal_events::DatadogTracesEncodingError,
    sinks::{datadog::traces::request_builder::DatadogTracesRequestBuilder, util::SinkBuilderExt},
};

#[derive(Default)]
struct EventPartitioner;

// Use all fields from the top level protobuf construct associated with the API key
#[derive(Hash, Eq, PartialEq, Clone, Debug)]
pub(crate) struct PartitionKey {
    pub(crate) api_key: Option<Arc<str>>,
    pub(crate) env: Option<String>,
    pub(crate) hostname: Option<String>,
    pub(crate) agent_version: Option<String>,
    // Those two last fields are configuration value and not a per-trace/span information, they come from the Datadog
    // trace-agent config directly: https://github.com/DataDog/datadog-agent/blob/0f73a78/pkg/trace/config/config.go#L293-L294
    pub(crate) target_tps: Option<i64>,
    pub(crate) error_tps: Option<i64>,
}

impl Partitioner for EventPartitioner {
    type Item = Event;
    type Key = PartitionKey;

    fn partition(&self, item: &Self::Item) -> Self::Key {
        match item {
            Event::Metric(_) => {
                panic!("unexpected metric");
            }
            Event::Log(_) => {
                panic!("unexpected log");
            }
            Event::Trace(t) => PartitionKey {
                api_key: item.metadata().datadog_api_key(),
                env: t
                    .get(event_path!("env"))
                    .map(|s| s.to_string_lossy().into_owned()),
                hostname: log_schema().host_key().and_then(|key| {
                    t.get((PathPrefix::Event, key))
                        .map(|s| s.to_string_lossy().into_owned())
                }),
                agent_version: t
                    .get(event_path!("agent_version"))
                    .map(|s| s.to_string_lossy().into_owned()),
                target_tps: t
                    .get(event_path!("target_tps"))
                    .and_then(|tps| tps.as_integer()),
                error_tps: t
                    .get(event_path!("error_tps"))
                    .and_then(|tps| tps.as_integer()),
            },
        }
    }
}

pub struct TracesSink<S> {
    service: S,
    request_builder: DatadogTracesRequestBuilder,
    batch_settings: BatcherSettings,
    shutdown: Sender<Sender<()>>,
    protocol: String,
    flusher: BoxFuture<'static, ()>,
}

const APM_FLUSH_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);

/// Runs the event driver and APM flusher with structured shutdown coordination.
///
/// Waits for the final flush acknowledgement even if the driver fails. Dropping this future drops
/// both child futures. The acknowledgement confirms a flush attempt, not Datadog delivery.
async fn run_driver_and_flusher<D, F>(
    driver: D,
    shutdown: Sender<Sender<()>>,
    flusher: F,
) -> Result<(), ()>
where
    D: Future<Output = Result<(), ()>>,
    F: Future<Output = ()>,
{
    run_driver_and_flusher_with_timeout(driver, shutdown, flusher, APM_FLUSH_SHUTDOWN_TIMEOUT).await
}

async fn run_driver_and_flusher_with_timeout<D, F>(
    driver: D,
    shutdown: Sender<Sender<()>>,
    flusher: F,
    shutdown_timeout: Duration,
) -> Result<(), ()>
where
    D: Future<Output = Result<(), ()>>,
    F: Future<Output = ()>,
{
    tokio::pin!(driver);
    tokio::pin!(flusher);

    let driver_result = tokio::select! {
        result = &mut driver => result,
        () = &mut flusher => return Err(()),
    };

    let (sender, receiver) = channel();
    _ = shutdown.send(sender);
    tokio::pin!(receiver);

    let ack_result = tokio::select! {
        result = &mut receiver => result.map_err(|_| ()),
        () = &mut flusher => (&mut receiver).await.map_err(|_| ()),
        _ = tokio::time::sleep(shutdown_timeout) => {
            warn!(
                message = "Timed out waiting for the Datadog APM stats flusher during shutdown.",
                timeout = ?shutdown_timeout,
            );
            Err(())
        }
    };

    // Preserve the existing contract: a missing acknowledgement fails an otherwise successful driver.
    driver_result.and(ack_result)
}

impl<S> TracesSink<S>
where
    S: Service<TraceApiRequest> + Send,
    S::Error: Debug + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse,
{
    pub fn new(
        service: S,
        request_builder: DatadogTracesRequestBuilder,
        batch_settings: BatcherSettings,
        shutdown: Sender<Sender<()>>,
        protocol: String,
        flusher: BoxFuture<'static, ()>,
    ) -> Self {
        TracesSink {
            service,
            request_builder,
            batch_settings,
            shutdown,
            protocol,
            flusher,
        }
    }

    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let TracesSink {
            service,
            request_builder,
            batch_settings,
            shutdown,
            protocol,
            flusher,
        } = *self;

        let driver = async move {
            input
                .batched_partitioned(EventPartitioner, batch_settings.timeout, |_| {
                    batch_settings.as_byte_size_config()
                })
                .incremental_request_builder(request_builder)
                .flat_map(stream::iter)
                .filter_map(|request| async move {
                    match request {
                        Err(e) => {
                            let (error_message, error_reason, dropped_events) = e.into_parts();
                            emit!(DatadogTracesEncodingError {
                                error_message,
                                error_reason,
                                dropped_events: dropped_events as usize,
                            });
                            None
                        }
                        Ok(req) => Some(req),
                    }
                })
                .into_driver(service)
                .protocol(protocol)
                .run()
                .await
        };

        run_driver_and_flusher(driver, shutdown, flusher).await
    }
}

#[async_trait]
impl<S> StreamSink<Event> for TracesSink<S>
where
    S: Service<TraceApiRequest> + Send,
    S::Error: Debug + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse,
{
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}

#[cfg(test)]
mod tests {
    use std::{
        future::Future,
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        task::{Context, Poll},
        time::Duration,
    };

    use futures_util::task::noop_waker_ref;
    use tokio::sync::oneshot;

    use super::{
        APM_FLUSH_SHUTDOWN_TIMEOUT, run_driver_and_flusher, run_driver_and_flusher_with_timeout,
    };

    async fn run_coordination_case(driver_result: Result<(), ()>) -> (Result<(), ()>, bool) {
        let (shutdown, shutdown_receiver) = oneshot::channel::<oneshot::Sender<()>>();
        let (force_flush, force_flush_observed) = oneshot::channel();
        let (allow_ack, allow_ack_receiver) = oneshot::channel();
        let flusher = async move {
            let ack = shutdown_receiver.await.expect("shutdown signal");
            force_flush.send(()).expect("force flush marker receiver");
            allow_ack_receiver.await.expect("ack release");
            ack.send(()).expect("shutdown ack receiver");
        };

        let coordination = tokio::spawn(run_driver_and_flusher(
            async move { driver_result },
            shutdown,
            flusher,
        ));
        let force_flush_observed = force_flush_observed.await.is_ok();
        assert!(!coordination.is_finished());
        allow_ack.send(()).expect("ack release receiver");

        let result = coordination.await.expect("coordination task");
        (result, force_flush_observed)
    }

    #[tokio::test]
    async fn driver_error_waits_for_force_flush_and_ack() {
        let (result, force_flush_observed) = run_coordination_case(Err(())).await;

        assert_eq!(result, Err(()));
        assert!(force_flush_observed);
    }

    #[tokio::test]
    async fn normal_driver_waits_for_force_flush_and_ack() {
        let (result, force_flush_observed) = run_coordination_case(Ok(())).await;

        assert_eq!(result, Ok(()));
        assert!(force_flush_observed);
    }

    #[tokio::test]
    async fn missing_shutdown_ack_fails_coordination() {
        let (shutdown, shutdown_receiver) = oneshot::channel::<oneshot::Sender<()>>();
        let flusher = async move {
            shutdown_receiver.await.expect("shutdown signal");
        };

        let result = run_driver_and_flusher_with_timeout(
            async { Ok(()) },
            shutdown,
            flusher,
            Duration::from_secs(1),
        )
        .await;

        assert_eq!(result, Err(()));
    }

    struct DropFlag(Arc<AtomicBool>);

    impl Drop for DropFlag {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    #[tokio::test(start_paused = true)]
    async fn shutdown_timeout_cancels_stuck_flusher_and_reports_coordination_failure() {
        let (shutdown, shutdown_receiver) = oneshot::channel::<oneshot::Sender<()>>();
        let (shutdown_observed, shutdown_observed_receiver) = oneshot::channel();
        let flusher_dropped = Arc::new(AtomicBool::new(false));
        let flusher_dropped_clone = Arc::clone(&flusher_dropped);
        let flusher = async move {
            let _drop_flag = DropFlag(flusher_dropped_clone);
            let _ack = shutdown_receiver.await.expect("shutdown signal");
            shutdown_observed
                .send(())
                .expect("shutdown observer receiver");
            std::future::pending::<()>().await;
        };

        let coordination = tokio::spawn(run_driver_and_flusher_with_timeout(
            async { Ok(()) },
            shutdown,
            flusher,
            APM_FLUSH_SHUTDOWN_TIMEOUT,
        ));

        shutdown_observed_receiver
            .await
            .expect("flusher should observe shutdown");
        tokio::time::advance(APM_FLUSH_SHUTDOWN_TIMEOUT + Duration::from_secs(1)).await;

        assert_eq!(coordination.await.expect("coordination task"), Err(()));
        assert!(flusher_dropped.load(Ordering::SeqCst));
    }

    struct PendingDriver(Arc<AtomicBool>);

    impl Future for PendingDriver {
        type Output = Result<(), ()>;

        fn poll(self: std::pin::Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
            Poll::Pending
        }
    }

    impl Drop for PendingDriver {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    struct PendingFlusher(Arc<AtomicBool>);

    impl Future for PendingFlusher {
        type Output = ();

        fn poll(self: std::pin::Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
            Poll::Pending
        }
    }

    impl Drop for PendingFlusher {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    #[test]
    fn dropping_coordination_future_drops_child_futures() {
        let (shutdown, _shutdown_receiver) = oneshot::channel();
        let driver_dropped = Arc::new(AtomicBool::new(false));
        let flusher_dropped = Arc::new(AtomicBool::new(false));
        let mut coordination = Box::pin(run_driver_and_flusher(
            PendingDriver(Arc::clone(&driver_dropped)),
            shutdown,
            PendingFlusher(Arc::clone(&flusher_dropped)),
        ));
        let mut context = Context::from_waker(noop_waker_ref());

        assert!(coordination.as_mut().poll(&mut context).is_pending());
        drop(coordination);

        assert!(driver_dropped.load(Ordering::SeqCst));
        assert!(flusher_dropped.load(Ordering::SeqCst));
    }
}
