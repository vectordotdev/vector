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
    semaphore: Option<Arc<Semaphore>>,
    timeout: Option<Duration>,
    metrics: Arc<RequestControlMetrics>,
}

impl RequestControl {
    pub(crate) fn new(concurrency_limit: Option<usize>, timeout: Option<Duration>) -> Self {
        Self {
            semaphore: concurrency_limit.map(|limit| Arc::new(Semaphore::new(limit))),
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
            timed_out: self.timeout.map_or_else(
                Counter::noop,
                |_| counter!(CounterName::ComponentTimedOutRequestsTotal, "protocol" => protocol),
            ),
            load_shed: self.semaphore.as_ref().map_or_else(
                Counter::noop,
                |_| counter!(CounterName::ComponentLoadShedRequestsTotal, "protocol" => protocol),
            ),
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
    concurrency_limit: Option<Gauge>,
}

impl RequestControlMetrics {
    #[expect(clippy::cast_precision_loss)]
    fn new(concurrency_limit: Option<usize>) -> Self {
        let concurrency_limit_gauge = concurrency_limit.map(|limit| {
            let gauge = gauge!(GaugeName::ComponentRequestConcurrencyLimit);
            gauge.set(limit as f64);
            gauge
        });

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
        // Enforce configured controls independently. Release any permit before finalizing the
        // acknowledgement, which is never subject to the request processing timeout.
        let control = self.control.clone();
        let timeout = control.timeout;
        let timed_out = self.timed_out.clone();
        let load_shed = self.load_shed.clone();
        let error_response = self.error_response.clone();
        let service = service_fn(move |request: Request<Body>| {
            let admitted = control
                .semaphore
                .as_ref()
                .map(|semaphore| Arc::clone(semaphore).try_acquire_owned())
                .transpose()
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
                        let processing = service.oneshot(request);
                        let result = match timeout {
                            Some(timeout) => tokio::time::timeout(timeout, processing).await,
                            None => Ok(processing.await),
                        };
                        match result {
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

    fn assert_metric_absent(name: &str) {
        assert!(
            vector_lib::metrics::Controller::get()
                .unwrap()
                .capture_metrics()
                .iter()
                .all(|metric| metric.name() != name),
            "disabled control must not register {name}"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn omitted_controls_allow_concurrent_requests_without_timeout() {
        init_metrics();
        let control = RequestControl::new(None, None);
        assert!(control.semaphore.is_none());
        let gate = Arc::new(Semaphore::new(0));
        let http = control
            .http_layer(http_error_response)
            .layer(gate_service(Arc::clone(&gate), || {
                Response::new(Body::empty())
            }));
        let grpc = control
            .grpc_layer(grpc_error_response)
            .layer(gate_service(Arc::clone(&gate), || {
                tonic::Status::new(tonic::Code::Ok, "").to_http()
            }));
        // Exceed the old default of 100 requests, and keep them pending beyond 30 seconds.
        let mut requests = Vec::new();
        for _ in 0..101 {
            let mut request = Box::pin(http.clone().oneshot(Request::new(Body::empty())));
            assert!(futures::poll!(&mut request).is_pending());
            requests.push(request);
        }
        let mut grpc_request = Box::pin(grpc.oneshot(Request::new(Body::empty())));
        assert!(futures::poll!(&mut grpc_request).is_pending());
        tokio::time::advance(Duration::from_secs(60)).await;
        for request in &mut requests {
            assert!(futures::poll!(request).is_pending());
        }
        assert!(futures::poll!(&mut grpc_request).is_pending());
        assert_eq!(active_requests(), 102.0);
        assert_metric_absent(GaugeName::ComponentRequestConcurrencyLimit.as_str());
        assert_metric_absent(CounterName::ComponentTimedOutRequestsTotal.as_str());
        assert_metric_absent(CounterName::ComponentLoadShedRequestsTotal.as_str());
        gate.add_permits(1);
        for request in requests {
            assert_eq!(request.await.unwrap().status(), StatusCode::OK);
        }
        assert_eq!(grpc_request.await.unwrap().headers()["grpc-status"], "0");
        assert_eq!(active_requests(), 0.0);
    }

    #[tokio::test(start_paused = true)]
    async fn concurrency_limit_without_timeout() {
        init_metrics();
        let control = RequestControl::new(Some(1), None);
        let gate = Arc::new(Semaphore::new(0));
        let http = control
            .http_layer(http_error_response)
            .layer(gate_service(Arc::clone(&gate), || {
                Response::new(Body::empty())
            }));
        let grpc = control
            .grpc_layer(grpc_error_response)
            .layer(gate_service(Arc::clone(&gate), || {
                tonic::Status::new(tonic::Code::Ok, "").to_http()
            }));
        let mut request = Box::pin(grpc.oneshot(Request::new(Body::empty())));
        assert!(futures::poll!(&mut request).is_pending());
        tokio::time::advance(Duration::from_secs(60)).await;
        assert!(futures::poll!(&mut request).is_pending());
        assert_eq!(active_requests(), 1.0);
        let rejected = http
            .clone()
            .oneshot(Request::new(Body::empty()))
            .await
            .unwrap();
        assert_eq!(rejected.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(
            counter_value(CounterName::ComponentLoadShedRequestsTotal, "http"),
            1.0
        );
        assert_metric_absent(CounterName::ComponentTimedOutRequestsTotal.as_str());
        drop(request);
        assert_eq!(active_requests(), 0.0);
        assert_eq!(control.semaphore.as_ref().unwrap().available_permits(), 1);
        gate.add_permits(1);
        assert_eq!(
            http.oneshot(Request::new(Body::empty()))
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
    }

    #[tokio::test(start_paused = true)]
    async fn timeout_without_concurrency_limit() {
        init_metrics();
        let control = RequestControl::new(None, Some(Duration::from_secs(1)));
        assert!(control.semaphore.is_none());
        let http = control
            .http_layer(http_error_response)
            .layer(UnreadyService);
        let grpc = control
            .grpc_layer(grpc_error_response)
            .layer(service_fn(|_: Request<Body>| {
                pending::<Result<Response<BoxBody>, Infallible>>()
            }));
        let mut first = Box::pin(http.clone().oneshot(Request::new(Body::empty())));
        let mut second = Box::pin(http.oneshot(Request::new(Body::empty())));
        let mut third = Box::pin(grpc.oneshot(Request::new(Body::empty())));
        assert!(futures::poll!(&mut first).is_pending());
        assert!(futures::poll!(&mut second).is_pending());
        assert!(futures::poll!(&mut third).is_pending());
        assert_eq!(active_requests(), 3.0);
        tokio::time::advance(Duration::from_secs(2)).await;
        assert_eq!(
            first.await.unwrap().status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
        assert_eq!(
            second.await.unwrap().status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
        assert_eq!(third.await.unwrap().headers()["grpc-status"], "14");
        assert_eq!(active_requests(), 0.0);
        assert_eq!(
            counter_value(CounterName::ComponentTimedOutRequestsTotal, "http"),
            2.0
        );
        assert_eq!(
            counter_value(CounterName::ComponentTimedOutRequestsTotal, "grpc"),
            1.0
        );
        assert_metric_absent(GaugeName::ComponentRequestConcurrencyLimit.as_str());
        assert_metric_absent(CounterName::ComponentLoadShedRequestsTotal.as_str());
    }

    #[tokio::test]
    async fn processing_error_releases_capacity() {
        init_metrics();
        let control = RequestControl::new(Some(1), Some(Duration::from_secs(5)));
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
        assert_eq!(control.semaphore.as_ref().unwrap().available_permits(), 1);
        assert_eq!(active_requests(), 0.0);
    }

    #[tokio::test]
    async fn http_and_grpc_share_capacity() {
        init_metrics();
        let control = RequestControl::new(Some(1), Some(Duration::from_secs(5)));
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
        assert_eq!(control.semaphore.as_ref().unwrap().available_permits(), 1);
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
        assert_eq!(control.semaphore.as_ref().unwrap().available_permits(), 1);
        let response = grpc.oneshot(Request::new(Body::empty())).await.unwrap();
        assert_eq!(response.headers()["grpc-status"], "0");
    }

    #[tokio::test(start_paused = true)]
    async fn timeout_and_cancellation_release_capacity() {
        init_metrics();
        let control = RequestControl::new(Some(1), Some(Duration::from_millis(20)));
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
        assert_eq!(control.semaphore.as_ref().unwrap().available_permits(), 1);
        assert_eq!(active_requests(), 0.0);

        let mut request = Box::pin(service.oneshot(Request::new(Body::empty())));
        assert!(futures::poll!(&mut request).is_pending());
        assert_eq!(active_requests(), 1.0);
        drop(request);
        assert_eq!(control.semaphore.as_ref().unwrap().available_permits(), 1);
        assert_eq!(active_requests(), 0.0);
    }

    #[tokio::test(start_paused = true)]
    async fn timeout_includes_waiting_for_readiness() {
        init_metrics();
        let control = RequestControl::new(Some(1), Some(Duration::from_secs(1)));
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
        assert_eq!(control.semaphore.as_ref().unwrap().available_permits(), 1);
        assert_eq!(active_requests(), 0.0);
        assert_eq!(
            counter_value(CounterName::ComponentTimedOutRequestsTotal, "http"),
            1.0
        );
    }

    #[tokio::test(start_paused = true)]
    async fn grpc_timeout_is_counted() {
        init_metrics();
        let control = RequestControl::new(Some(1), Some(Duration::from_secs(1)));
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
