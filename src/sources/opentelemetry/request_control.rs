use std::{convert::Infallible, sync::Arc, time::Duration};

use http::{Request, Response};
use hyper::Body;
use metrics::{Counter, Gauge};
use tokio::sync::Semaphore;
use tower::{Layer, Service, ServiceExt, service_fn, util::BoxCloneService};
use vector_lib::{
    counter,
    event::{BatchStatus, BatchStatusReceiver},
    gauge,
    internal_event::{CounterName, GaugeName},
};

/// Concurrency limit shared by all HTTP and gRPC requests handled by one OTLP source.
#[derive(Clone)]
pub(crate) struct RequestControl {
    semaphore: Arc<Semaphore>,
    timeout: Duration,
    metrics: Arc<RequestControlMetrics>,
}

impl RequestControl {
    pub(crate) fn new(concurrency_limit: usize, timeout: Duration) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(concurrency_limit)),
            timeout,
            metrics: Arc::new(RequestControlMetrics::new(concurrency_limit)),
        }
    }

    pub(crate) fn http_layer<R>(&self, error_response: R) -> RequestControlLayer<R> {
        self.layer(error_response, "http")
    }

    pub(crate) fn grpc_layer<R>(&self, error_response: R) -> RequestControlLayer<R> {
        self.layer(error_response, "grpc")
    }

    fn layer<R>(&self, error_response: R, protocol: &'static str) -> RequestControlLayer<R> {
        RequestControlLayer {
            control: self.clone(),
            timed_out: counter!(CounterName::ComponentTimedOutRequestsTotal, "protocol" => protocol),
            load_shed: counter!(CounterName::ComponentLoadShedRequestsTotal, "protocol" => protocol),
            error_response,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum MiddlewareError {
    LoadShed,
    TimedOut,
    Unavailable,
}

impl MiddlewareError {
    pub(crate) const fn message(self) -> &'static str {
        match self {
            Self::LoadShed => "OTLP request limit exceeded",
            Self::TimedOut => "OTLP request timed out",
            Self::Unavailable => "OTLP request unavailable",
        }
    }
}

struct RequestControlMetrics {
    active_level: Gauge,
    #[expect(
        dead_code,
        reason = "retain the concurrency limit gauge handle for the controller lifetime"
    )]
    concurrency_limit: Gauge,
}

impl RequestControlMetrics {
    #[expect(clippy::cast_precision_loss)]
    fn new(concurrency_limit: usize) -> Self {
        let concurrency_limit_gauge = gauge!(GaugeName::ComponentRequestConcurrencyLimit);
        concurrency_limit_gauge.set(concurrency_limit as f64);

        Self {
            active_level: gauge!(GaugeName::ComponentRequestActive),
            concurrency_limit: concurrency_limit_gauge,
        }
    }

    fn active_token(&self) -> ActiveRequestGuard {
        let gauge = self.active_level.clone();
        gauge.increment(1.0);
        ActiveRequestGuard(gauge)
    }
}

struct ActiveRequestGuard(Gauge);

impl Drop for ActiveRequestGuard {
    fn drop(&mut self) {
        self.0.decrement(1.0);
    }
}

pub(crate) struct PendingAcknowledgement<B> {
    receiver: BatchStatusReceiver,
    failure_response: fn(AcknowledgementFailure) -> Response<B>,
}

impl<B> PendingAcknowledgement<B> {
    pub(crate) const fn new(
        receiver: BatchStatusReceiver,
        failure_response: fn(AcknowledgementFailure) -> Response<B>,
    ) -> Self {
        Self {
            receiver,
            failure_response,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum AcknowledgementFailure {
    Errored,
    Rejected,
}

impl AcknowledgementFailure {
    const fn from_status(status: BatchStatus) -> Option<Self> {
        match status {
            BatchStatus::Delivered => None,
            BatchStatus::Errored => Some(Self::Errored),
            BatchStatus::Rejected => Some(Self::Rejected),
        }
    }
}

async fn finalize_acknowledgement<B>(mut response: Response<B>) -> Result<Response<B>, Infallible>
where
    B: 'static,
{
    if let Some(pending) = response
        .extensions_mut()
        .remove::<PendingAcknowledgement<B>>()
        && let Some(status) = AcknowledgementFailure::from_status(pending.receiver.await)
    {
        response = (pending.failure_response)(status);
    }

    Ok(response)
}

#[derive(Clone)]
pub(crate) struct RequestControlLayer<R> {
    control: RequestControl,
    timed_out: Counter,
    load_shed: Counter,
    error_response: R,
}

impl<S, R, B> Layer<S> for RequestControlLayer<R>
where
    S: Service<Request<Body>, Response = Response<B>> + Clone + Send + 'static,
    B: Send + 'static,
    S::Error: std::fmt::Display,
    S::Future: Send + 'static,
    R: Fn(MiddlewareError) -> Response<B> + Clone + Send + 'static,
{
    type Service = BoxCloneService<Request<Body>, Response<B>, Infallible>;

    fn layer(&self, service: S) -> Self::Service {
        // Acquire shared capacity immediately, reject requests when none is available, and release
        // the permit before finalizing the acknowledgement without a timeout.
        let control = self.control.clone();
        let timeout = control.timeout;
        let timed_out = self.timed_out.clone();
        let load_shed = self.load_shed.clone();
        let error_response = self.error_response.clone();
        let service = service_fn(move |request: Request<Body>| {
            let admitted = Arc::clone(&control.semaphore)
                .try_acquire_owned()
                .map(|permit| (permit, control.metrics.active_token(), service.clone()))
                .map_err(|_| {
                    load_shed.increment(1);
                    MiddlewareError::LoadShed
                });
            let timed_out = timed_out.clone();
            let error_response = error_response.clone();

            async move {
                let response = match admitted {
                    Ok((_permit, _active, service)) => {
                        match tokio::time::timeout(timeout, service.oneshot(request)).await {
                            Ok(Ok(response)) => response,
                            Ok(Err(error)) => {
                                error!(message = "OTLP request middleware failed.", %error);
                                error_response(MiddlewareError::Unavailable)
                            }
                            Err(_) => {
                                timed_out.increment(1);
                                error_response(MiddlewareError::TimedOut)
                            }
                        }
                    }
                    Err(error) => error_response(error),
                };

                Ok::<_, Infallible>(response)
            }
        });
        let service = service.and_then(finalize_acknowledgement);

        BoxCloneService::new(service)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        future::{Ready, pending, ready},
        task::{Context, Poll},
    };

    use http::StatusCode;
    use hyper::body::HttpBody;
    use prost::Message;
    use tonic::body::BoxBody;

    use super::*;
    use crate::sources::opentelemetry::{
        grpc::middleware_error_response as grpc_error_response,
        http::middleware_error_response as http_error_response, status::Status,
    };

    fn gate_service<R: Send + 'static>(
        gate: Arc<Semaphore>,
        response: fn() -> R,
    ) -> impl Service<Request<Body>, Response = R, Error = Infallible, Future: Send> + Clone {
        service_fn(move |_: Request<Body>| {
            let gate = Arc::clone(&gate);
            async move {
                let _permit = gate.acquire().await.unwrap();
                Ok(response())
            }
        })
    }

    #[derive(Clone)]
    struct UnreadyService;

    impl Service<Request<Body>> for UnreadyService {
        type Response = Response<Body>;
        type Error = Infallible;
        type Future = Ready<Result<Self::Response, Self::Error>>;

        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Pending
        }

        fn call(&mut self, _request: Request<Body>) -> Self::Future {
            panic!("an unready service must not be called")
        }
    }

    fn init_metrics() {
        vector_lib::metrics::init_test();
        vector_lib::metrics::Controller::get().unwrap().reset();
    }

    fn counter_value(name: CounterName, protocol: &str) -> f64 {
        let metrics = vector_lib::metrics::Controller::get()
            .unwrap()
            .capture_metrics();
        let metric = metrics
            .iter()
            .find(|metric| {
                metric.name() == name.as_str()
                    && metric.tag_value("protocol").as_deref() == Some(protocol)
            })
            .expect("request counter must be registered with its protocol");
        match metric.value() {
            vector_lib::event::MetricValue::Counter { value } => *value,
            _ => panic!("request metric must be a counter"),
        }
    }

    fn active_requests() -> f64 {
        let metrics = vector_lib::metrics::Controller::get()
            .unwrap()
            .capture_metrics();
        let metric = metrics
            .iter()
            .find(|metric| metric.name() == GaugeName::ComponentRequestActive.as_str())
            .expect("active request gauge must be registered");
        match metric.value() {
            vector_lib::event::MetricValue::Gauge { value } => *value,
            _ => panic!("active request metric must be a gauge"),
        }
    }

    #[tokio::test]
    async fn processing_error_releases_capacity() {
        init_metrics();
        let control = RequestControl::new(1, Duration::from_secs(5));
        let service =
            control
                .http_layer(http_error_response)
                .layer(service_fn(|_: Request<Body>| {
                    ready(Err::<Response<Body>, _>(std::io::Error::other(
                        "request failed",
                    )))
                }));

        let response = service.oneshot(Request::new(Body::empty())).await.unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(control.semaphore.available_permits(), 1);
        assert_eq!(active_requests(), 0.0);
    }

    #[tokio::test]
    async fn http_and_grpc_share_capacity() {
        init_metrics();
        let control = RequestControl::new(1, Duration::from_secs(5));
        let gate = Arc::new(Semaphore::new(0));
        let mut http = control
            .http_layer(http_error_response)
            .layer(gate_service(Arc::clone(&gate), || {
                Response::new(Body::empty())
            }));
        let mut grpc = control
            .grpc_layer(grpc_error_response)
            .layer(gate_service(Arc::clone(&gate), || {
                tonic::Status::new(tonic::Code::Ok, "").to_http()
            }));

        // Idle connections must not reserve capacity by polling readiness.
        http.ready().await.unwrap();
        grpc.ready().await.unwrap();
        assert_eq!(control.semaphore.available_permits(), 1);
        assert_eq!(active_requests(), 0.0);

        let mut processing = Box::pin(http.clone().oneshot(Request::new(Body::empty())));
        assert!(futures::poll!(&mut processing).is_pending());
        assert_eq!(active_requests(), 1.0);

        let rejected = http.oneshot(Request::new(Body::empty())).await.unwrap();
        assert_eq!(rejected.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(
            counter_value(CounterName::ComponentLoadShedRequestsTotal, "http"),
            1.0
        );
        assert_eq!(
            counter_value(CounterName::ComponentLoadShedRequestsTotal, "grpc"),
            0.0
        );

        let rejected = grpc
            .clone()
            .oneshot(Request::new(Body::empty()))
            .await
            .unwrap();
        assert_eq!(rejected.headers()["grpc-status"], "14");
        assert_eq!(
            counter_value(CounterName::ComponentLoadShedRequestsTotal, "grpc"),
            1.0
        );
        assert_eq!(active_requests(), 1.0);

        gate.add_permits(1);
        assert_eq!(processing.await.unwrap().status(), StatusCode::OK);
        assert_eq!(active_requests(), 0.0);
        assert_eq!(control.semaphore.available_permits(), 1);
        let response = grpc.oneshot(Request::new(Body::empty())).await.unwrap();
        assert_eq!(response.headers()["grpc-status"], "0");
    }

    #[tokio::test(start_paused = true)]
    async fn timeout_and_cancellation_release_capacity() {
        init_metrics();
        let control = RequestControl::new(1, Duration::from_millis(20));
        let service =
            control
                .http_layer(http_error_response)
                .layer(service_fn(|_: Request<Body>| {
                    pending::<Result<Response<Body>, Infallible>>()
                }));

        let timed_out = service
            .clone()
            .oneshot(Request::new(Body::empty()))
            .await
            .unwrap();
        assert_eq!(timed_out.status(), StatusCode::SERVICE_UNAVAILABLE);
        let bytes = timed_out.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(
            Status::decode(bytes).unwrap().code,
            tonic::Code::Unavailable as i32
        );
        assert_eq!(
            counter_value(CounterName::ComponentTimedOutRequestsTotal, "http"),
            1.0
        );
        assert_eq!(control.semaphore.available_permits(), 1);
        assert_eq!(active_requests(), 0.0);

        let mut request = Box::pin(service.oneshot(Request::new(Body::empty())));
        assert!(futures::poll!(&mut request).is_pending());
        assert_eq!(active_requests(), 1.0);
        drop(request);
        assert_eq!(control.semaphore.available_permits(), 1);
        assert_eq!(active_requests(), 0.0);
    }

    #[tokio::test(start_paused = true)]
    async fn timeout_includes_waiting_for_readiness() {
        init_metrics();
        let control = RequestControl::new(1, Duration::from_secs(1));
        let service = control
            .http_layer(http_error_response)
            .layer(UnreadyService);

        // The outer timeout ensures that missing readiness coverage fails instead of hanging.
        let response = tokio::time::timeout(
            Duration::from_secs(2),
            service.oneshot(Request::new(Body::empty())),
        )
        .await
        .expect("readiness must be covered by the request timeout")
        .unwrap();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(control.semaphore.available_permits(), 1);
        assert_eq!(active_requests(), 0.0);
        assert_eq!(
            counter_value(CounterName::ComponentTimedOutRequestsTotal, "http"),
            1.0
        );
    }

    #[tokio::test(start_paused = true)]
    async fn grpc_timeout_is_counted() {
        init_metrics();
        let control = RequestControl::new(1, Duration::from_secs(1));
        let service =
            control
                .grpc_layer(grpc_error_response)
                .layer(service_fn(|_: Request<Body>| {
                    pending::<Result<Response<BoxBody>, Infallible>>()
                }));

        let response = service.oneshot(Request::new(Body::empty())).await.unwrap();
        assert_eq!(response.headers()["grpc-status"], "14");
        assert_eq!(
            counter_value(CounterName::ComponentTimedOutRequestsTotal, "grpc"),
            1.0
        );
    }
}
