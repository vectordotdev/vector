use std::{
    convert::Infallible,
    fmt::Display,
    marker::PhantomData,
    sync::{Arc, Mutex, PoisonError},
    time::Duration,
};

use metrics::{Counter, Gauge};
use tokio::sync::Semaphore;
use tower::{Layer, Service, ServiceExt, service_fn, util::BoxCloneService};
use vector_lib::{
    counter,
    event::{BatchStatus, BatchStatusReceiver},
    gauge,
    internal_event::{CounterName, GaugeName},
};

/// Request served by the legacy Warp HTTP listener.
pub(crate) type HttpRequest = http::Request<hyper::Body>;

/// Request served by the native Tonic gRPC listener.
pub(crate) type GrpcRequest = http_1::Request<tonic::body::BoxBody>;

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

    pub(crate) fn http_layer<R>(&self, error_response: R) -> RequestControlLayer<R, HttpRequest> {
        self.layer(error_response, "http")
    }

    pub(crate) fn grpc_layer<R>(&self, error_response: R) -> RequestControlLayer<R, GrpcRequest> {
        self.layer(error_response, "grpc")
    }

    fn layer<R, Req>(
        &self,
        error_response: R,
        protocol: &'static str,
    ) -> RequestControlLayer<R, Req> {
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
            request: PhantomData,
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

/// Acknowledgement awaited after request processing, replacing the response when delivery fails.
pub(crate) struct PendingAcknowledgement<Resp> {
    receiver: BatchStatusReceiver,
    failure_response: fn(AcknowledgementFailure) -> Resp,
}

impl<Resp> PendingAcknowledgement<Resp> {
    pub(crate) const fn new(
        receiver: BatchStatusReceiver,
        failure_response: fn(AcknowledgementFailure) -> Resp,
    ) -> Self {
        Self {
            receiver,
            failure_response,
        }
    }
}

/// Native HTTP 1 extensions must be `Clone`, but a pending acknowledgement owns its single
/// receiver. Clones share that receiver: request control takes it at most once, and dropping every
/// clone before then drops the receiver.
pub(crate) struct SharedPendingAcknowledgement<Resp>(
    Arc<Mutex<Option<PendingAcknowledgement<Resp>>>>,
);

impl<Resp> SharedPendingAcknowledgement<Resp> {
    pub(crate) fn new(pending: PendingAcknowledgement<Resp>) -> Self {
        Self(Arc::new(Mutex::new(Some(pending))))
    }

    fn take(&self) -> Option<PendingAcknowledgement<Resp>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).take()
    }
}

impl<Resp> Clone for SharedPendingAcknowledgement<Resp> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

/// Response that may carry a pending acknowledgement out of request processing.
pub(crate) trait AcknowledgementResponse: Sized {
    fn take_pending_acknowledgement(&mut self) -> Option<PendingAcknowledgement<Self>>;
}

/// Legacy HTTP extensions own the pending acknowledgement directly.
impl<B: 'static> AcknowledgementResponse for http::Response<B> {
    fn take_pending_acknowledgement(&mut self) -> Option<PendingAcknowledgement<Self>> {
        self.extensions_mut().remove()
    }
}

/// Native gRPC responses carry the pending acknowledgement in a cloneable shared slot.
impl<B: 'static> AcknowledgementResponse for http_1::Response<B> {
    fn take_pending_acknowledgement(&mut self) -> Option<PendingAcknowledgement<Self>> {
        self.extensions_mut()
            .remove::<SharedPendingAcknowledgement<Self>>()?
            .take()
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

async fn finalize_acknowledgement<Resp>(mut response: Resp) -> Result<Resp, Infallible>
where
    Resp: AcknowledgementResponse,
{
    if let Some(pending) = response.take_pending_acknowledgement()
        && let Some(status) = AcknowledgementFailure::from_status(pending.receiver.await)
    {
        response = (pending.failure_response)(status);
    }

    Ok(response)
}

pub(crate) struct RequestControlLayer<R, Req> {
    control: RequestControl,
    timed_out: Counter,
    load_shed: Counter,
    error_response: R,
    request: PhantomData<fn(Req)>,
}

impl<R: Clone, Req> Clone for RequestControlLayer<R, Req> {
    fn clone(&self) -> Self {
        Self {
            control: self.control.clone(),
            timed_out: self.timed_out.clone(),
            load_shed: self.load_shed.clone(),
            error_response: self.error_response.clone(),
            request: PhantomData,
        }
    }
}

impl<S, R, Req, Resp> Layer<S> for RequestControlLayer<R, Req>
where
    S: Service<Req, Response = Resp> + Clone + Send + 'static,
    S::Error: Display,
    S::Future: Send + 'static,
    Req: Send + 'static,
    Resp: AcknowledgementResponse + Send + 'static,
    R: Fn(MiddlewareError) -> Resp + Clone + Send + 'static,
{
    type Service = BoxCloneService<Req, Resp, Infallible>;

    fn layer(&self, service: S) -> Self::Service {
        // Enforce configured controls independently. Release any permit before finalizing the
        // acknowledgement, which is never subject to the request processing timeout.
        let control = self.control.clone();
        let timeout = control.timeout;
        let timed_out = self.timed_out.clone();
        let load_shed = self.load_shed.clone();
        let error_response = self.error_response.clone();
        let service = service_fn(move |request: Req| {
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
    use hyper::{Body, body::HttpBody};
    use prost::Message;
    use tonic::body::{BoxBody, empty_body};
    use vector_lib::event::{BatchNotifier, Event, EventStatus, LogEvent};

    use super::*;
    use crate::sources::opentelemetry::{
        grpc::middleware_error_response as grpc_error_response,
        http::middleware_error_response as http_error_response, status::Status,
    };

    type HttpResponse = http::Response<Body>;
    type GrpcResponse = http_1::Response<BoxBody>;

    fn http_request() -> HttpRequest {
        http::Request::new(Body::empty())
    }

    fn grpc_request() -> GrpcRequest {
        http_1::Request::new(empty_body())
    }

    fn http_ok() -> HttpResponse {
        http::Response::new(Body::empty())
    }

    fn grpc_ok() -> GrpcResponse {
        tonic::Status::new(tonic::Code::Ok, "").into_http()
    }

    fn gate_service<Req: Send + 'static, Resp: Send + 'static>(
        gate: Arc<Semaphore>,
        response: fn() -> Resp,
    ) -> impl Service<Req, Response = Resp, Error = Infallible, Future: Send> + Clone {
        service_fn(move |_: Req| {
            let gate = Arc::clone(&gate);
            async move {
                let _permit = gate.acquire().await.unwrap();
                Ok(response())
            }
        })
    }

    #[derive(Clone)]
    struct UnreadyService;

    impl Service<HttpRequest> for UnreadyService {
        type Response = HttpResponse;
        type Error = Infallible;
        type Future = Ready<Result<Self::Response, Self::Error>>;

        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Pending
        }

        fn call(&mut self, _request: HttpRequest) -> Self::Future {
            panic!("an unready service must not be called")
        }
    }

    /// Returns an event whose finalization completes the returned acknowledgement receiver.
    fn acknowledged_event() -> (Event, BatchStatusReceiver) {
        let mut events = [Event::Log(LogEvent::from("message"))];
        let receiver = BatchNotifier::apply_to(&mut events);
        let [event] = events;
        (event, receiver)
    }

    fn finalize(event: Event, status: EventStatus) {
        event.metadata().finalizers().update_status(status);
        drop(event);
    }

    // Distinct responses identify which acknowledgement outcome replaced the processed response.
    fn http_acknowledgement_failure(failure: AcknowledgementFailure) -> HttpResponse {
        let status = match failure {
            AcknowledgementFailure::Errored => StatusCode::INTERNAL_SERVER_ERROR,
            AcknowledgementFailure::Rejected => StatusCode::BAD_GATEWAY,
        };
        http::Response::builder()
            .status(status)
            .body(Body::empty())
            .unwrap()
    }

    fn grpc_acknowledgement_failure(failure: AcknowledgementFailure) -> GrpcResponse {
        match failure {
            AcknowledgementFailure::Errored => tonic::Status::internal(""),
            AcknowledgementFailure::Rejected => tonic::Status::data_loss(""),
        }
        .into_http()
    }

    fn http_request_awaiting(receiver: BatchStatusReceiver) -> HttpRequest {
        let mut request = http_request();
        request.extensions_mut().insert(PendingAcknowledgement::new(
            receiver,
            http_acknowledgement_failure,
        ));
        request
    }

    fn grpc_request_awaiting(receiver: BatchStatusReceiver) -> GrpcRequest {
        let mut request = grpc_request();
        request
            .extensions_mut()
            .insert(SharedPendingAcknowledgement::new(
                PendingAcknowledgement::new(receiver, grpc_acknowledgement_failure),
            ));
        request
    }

    /// Completes processing immediately, handing any request acknowledgement to its response as
    /// the OTLP HTTP handlers do.
    fn http_acknowledging_service()
    -> impl Service<HttpRequest, Response = HttpResponse, Error = Infallible, Future: Send> + Clone
    {
        service_fn(|mut request: HttpRequest| {
            let mut response = http_ok();
            if let Some(pending) = request
                .extensions_mut()
                .remove::<PendingAcknowledgement<HttpResponse>>()
            {
                response.extensions_mut().insert(pending);
            }
            ready(Ok::<_, Infallible>(response))
        })
    }

    /// Completes processing immediately, handing any request acknowledgement to its response as
    /// the OTLP gRPC services do.
    fn grpc_acknowledging_service()
    -> impl Service<GrpcRequest, Response = GrpcResponse, Error = Infallible, Future: Send> + Clone
    {
        service_fn(|mut request: GrpcRequest| {
            let mut response = grpc_ok();
            if let Some(pending) = request
                .extensions_mut()
                .remove::<SharedPendingAcknowledgement<GrpcResponse>>()
            {
                response.extensions_mut().insert(pending);
            }
            ready(Ok::<_, Infallible>(response))
        })
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

    #[expect(
        clippy::float_cmp,
        reason = "These deterministic fixture values are compared exactly."
    )]
    #[tokio::test(start_paused = true)]
    async fn omitted_controls_allow_concurrent_requests_without_timeout() {
        init_metrics();
        let control = RequestControl::new(None, None);
        assert!(control.semaphore.is_none());
        let gate = Arc::new(Semaphore::new(0));
        let http = control
            .http_layer(http_error_response)
            .layer(gate_service::<HttpRequest, _>(Arc::clone(&gate), http_ok));
        let grpc = control
            .grpc_layer(grpc_error_response)
            .layer(gate_service::<GrpcRequest, _>(Arc::clone(&gate), grpc_ok));
        // Exceed the old default of 100 requests, and keep them pending beyond 30 seconds.
        let mut requests = Vec::new();
        for _ in 0..101 {
            let mut request = Box::pin(http.clone().oneshot(http_request()));
            assert!(futures::poll!(&mut request).is_pending());
            requests.push(request);
        }
        let mut grpc_pending = Box::pin(grpc.oneshot(grpc_request()));
        assert!(futures::poll!(&mut grpc_pending).is_pending());
        tokio::time::advance(Duration::from_mins(1)).await;
        for request in &mut requests {
            assert!(futures::poll!(request).is_pending());
        }
        assert!(futures::poll!(&mut grpc_pending).is_pending());
        assert_eq!(active_requests(), 102.0);
        assert_metric_absent(GaugeName::ComponentRequestConcurrencyLimit.as_str());
        assert_metric_absent(CounterName::ComponentTimedOutRequestsTotal.as_str());
        assert_metric_absent(CounterName::ComponentLoadShedRequestsTotal.as_str());
        gate.add_permits(1);
        for request in requests {
            assert_eq!(request.await.unwrap().status(), StatusCode::OK);
        }
        assert_eq!(grpc_pending.await.unwrap().headers()["grpc-status"], "0");
        assert_eq!(active_requests(), 0.0);
    }

    #[expect(
        clippy::float_cmp,
        reason = "These deterministic fixture values are compared exactly."
    )]
    #[tokio::test(start_paused = true)]
    async fn concurrency_limit_without_timeout() {
        init_metrics();
        let control = RequestControl::new(Some(1), None);
        let gate = Arc::new(Semaphore::new(0));
        let http = control
            .http_layer(http_error_response)
            .layer(gate_service::<HttpRequest, _>(Arc::clone(&gate), http_ok));
        let grpc = control
            .grpc_layer(grpc_error_response)
            .layer(gate_service::<GrpcRequest, _>(Arc::clone(&gate), grpc_ok));
        let mut request = Box::pin(grpc.oneshot(grpc_request()));
        assert!(futures::poll!(&mut request).is_pending());
        tokio::time::advance(Duration::from_mins(1)).await;
        assert!(futures::poll!(&mut request).is_pending());
        assert_eq!(active_requests(), 1.0);
        let rejected = http.clone().oneshot(http_request()).await.unwrap();
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
            http.oneshot(http_request()).await.unwrap().status(),
            StatusCode::OK
        );
    }

    #[expect(
        clippy::float_cmp,
        reason = "These deterministic fixture values are compared exactly."
    )]
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
            .layer(service_fn(|_: GrpcRequest| {
                pending::<Result<GrpcResponse, Infallible>>()
            }));
        let mut first = Box::pin(http.clone().oneshot(http_request()));
        let mut second = Box::pin(http.oneshot(http_request()));
        let mut third = Box::pin(grpc.oneshot(grpc_request()));
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

    #[expect(
        clippy::float_cmp,
        reason = "These deterministic fixture values are compared exactly."
    )]
    #[tokio::test]
    async fn processing_error_releases_capacity() {
        init_metrics();
        let control = RequestControl::new(Some(1), Some(Duration::from_secs(5)));
        let http = control
            .http_layer(http_error_response)
            .layer(service_fn(|_: HttpRequest| {
                ready(Err::<HttpResponse, _>(std::io::Error::other(
                    "request failed",
                )))
            }));
        // Native Tonic routes report failures as boxed errors rather than `Infallible`.
        let grpc = control
            .grpc_layer(grpc_error_response)
            .layer(service_fn(|_: GrpcRequest| {
                ready(Err::<GrpcResponse, tower::BoxError>(
                    std::io::Error::other("request failed").into(),
                ))
            }));

        let response = http.oneshot(http_request()).await.unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(control.semaphore.as_ref().unwrap().available_permits(), 1);
        assert_eq!(active_requests(), 0.0);

        let response = grpc.oneshot(grpc_request()).await.unwrap();
        assert_eq!(response.headers()["grpc-status"], "14");
        assert_eq!(control.semaphore.as_ref().unwrap().available_permits(), 1);
        assert_eq!(active_requests(), 0.0);
    }

    #[expect(
        clippy::float_cmp,
        reason = "These deterministic fixture values are compared exactly."
    )]
    #[tokio::test]
    async fn http_and_grpc_share_capacity() {
        init_metrics();
        let control = RequestControl::new(Some(1), Some(Duration::from_secs(5)));
        let gate = Arc::new(Semaphore::new(0));
        let mut http = control
            .http_layer(http_error_response)
            .layer(gate_service::<HttpRequest, _>(Arc::clone(&gate), http_ok));
        let mut grpc = control
            .grpc_layer(grpc_error_response)
            .layer(gate_service::<GrpcRequest, _>(Arc::clone(&gate), grpc_ok));

        // Idle connections must not reserve capacity by polling readiness.
        http.ready().await.unwrap();
        grpc.ready().await.unwrap();
        assert_eq!(control.semaphore.as_ref().unwrap().available_permits(), 1);
        assert_eq!(active_requests(), 0.0);

        let mut processing = Box::pin(http.clone().oneshot(http_request()));
        assert!(futures::poll!(&mut processing).is_pending());
        assert_eq!(active_requests(), 1.0);

        let rejected = http.oneshot(http_request()).await.unwrap();
        assert_eq!(rejected.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(
            counter_value(CounterName::ComponentLoadShedRequestsTotal, "http"),
            1.0
        );
        assert_eq!(
            counter_value(CounterName::ComponentLoadShedRequestsTotal, "grpc"),
            0.0
        );

        let rejected = grpc.clone().oneshot(grpc_request()).await.unwrap();
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
        let response = grpc.oneshot(grpc_request()).await.unwrap();
        assert_eq!(response.headers()["grpc-status"], "0");
    }

    #[expect(
        clippy::float_cmp,
        reason = "These deterministic fixture values are compared exactly."
    )]
    #[tokio::test(start_paused = true)]
    async fn timeout_and_cancellation_release_capacity() {
        init_metrics();
        let control = RequestControl::new(Some(1), Some(Duration::from_millis(20)));
        let http = control
            .http_layer(http_error_response)
            .layer(service_fn(|_: HttpRequest| {
                pending::<Result<HttpResponse, Infallible>>()
            }));
        let grpc = control
            .grpc_layer(grpc_error_response)
            .layer(service_fn(|_: GrpcRequest| {
                pending::<Result<GrpcResponse, Infallible>>()
            }));

        let timed_out = http.clone().oneshot(http_request()).await.unwrap();
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

        let mut request = Box::pin(http.oneshot(http_request()));
        assert!(futures::poll!(&mut request).is_pending());
        assert_eq!(active_requests(), 1.0);
        drop(request);
        assert_eq!(control.semaphore.as_ref().unwrap().available_permits(), 1);
        assert_eq!(active_requests(), 0.0);

        let mut request = Box::pin(grpc.oneshot(grpc_request()));
        assert!(futures::poll!(&mut request).is_pending());
        assert_eq!(active_requests(), 1.0);
        drop(request);
        assert_eq!(control.semaphore.as_ref().unwrap().available_permits(), 1);
        assert_eq!(active_requests(), 0.0);
    }

    #[expect(
        clippy::float_cmp,
        reason = "These deterministic fixture values are compared exactly."
    )]
    #[tokio::test(start_paused = true)]
    async fn timeout_includes_waiting_for_readiness() {
        init_metrics();
        let control = RequestControl::new(Some(1), Some(Duration::from_secs(1)));
        let service = control
            .http_layer(http_error_response)
            .layer(UnreadyService);

        // The outer timeout ensures that missing readiness coverage fails instead of hanging.
        let response =
            tokio::time::timeout(Duration::from_secs(2), service.oneshot(http_request()))
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

    #[expect(
        clippy::float_cmp,
        reason = "These deterministic fixture values are compared exactly."
    )]
    #[tokio::test(start_paused = true)]
    async fn grpc_timeout_is_counted() {
        init_metrics();
        let control = RequestControl::new(Some(1), Some(Duration::from_secs(1)));
        let service =
            control
                .grpc_layer(grpc_error_response)
                .layer(service_fn(|_: GrpcRequest| {
                    pending::<Result<GrpcResponse, Infallible>>()
                }));

        let response = service.oneshot(grpc_request()).await.unwrap();
        assert_eq!(response.headers()["grpc-status"], "14");
        assert_eq!(
            counter_value(CounterName::ComponentTimedOutRequestsTotal, "grpc"),
            1.0
        );
    }

    #[expect(
        clippy::float_cmp,
        reason = "These deterministic fixture values are compared exactly."
    )]
    #[tokio::test(start_paused = true)]
    async fn acknowledgement_outcomes_are_awaited_after_admission_and_outside_timeout() {
        init_metrics();
        let control = RequestControl::new(Some(1), Some(Duration::from_secs(1)));
        let http = control
            .http_layer(http_error_response)
            .layer(http_acknowledging_service());
        let grpc = control
            .grpc_layer(grpc_error_response)
            .layer(grpc_acknowledging_service());

        for (status, http_status, grpc_status) in [
            (EventStatus::Delivered, StatusCode::OK, "0"),
            (
                EventStatus::Errored,
                StatusCode::INTERNAL_SERVER_ERROR,
                "13",
            ),
            (EventStatus::Rejected, StatusCode::BAD_GATEWAY, "15"),
        ] {
            let (http_event, receiver) = acknowledged_event();
            let mut http_response = Box::pin(http.clone().oneshot(http_request_awaiting(receiver)));
            assert!(futures::poll!(&mut http_response).is_pending());
            // A request awaiting acknowledgement holds neither the permit nor the active gauge, so
            // the other protocol is admitted with a limit of one.
            assert_eq!(control.semaphore.as_ref().unwrap().available_permits(), 1);
            assert_eq!(active_requests(), 0.0);

            let (grpc_event, receiver) = acknowledged_event();
            let mut grpc_response = Box::pin(grpc.clone().oneshot(grpc_request_awaiting(receiver)));
            assert!(futures::poll!(&mut grpc_response).is_pending());
            assert_eq!(control.semaphore.as_ref().unwrap().available_permits(), 1);
            assert_eq!(active_requests(), 0.0);

            tokio::time::advance(Duration::from_secs(2)).await;
            assert!(futures::poll!(&mut http_response).is_pending());
            assert!(futures::poll!(&mut grpc_response).is_pending());

            finalize(http_event, status);
            finalize(grpc_event, status);
            assert_eq!(http_response.await.unwrap().status(), http_status);
            assert_eq!(
                grpc_response.await.unwrap().headers()["grpc-status"],
                grpc_status
            );
        }

        for protocol in ["http", "grpc"] {
            assert_eq!(
                counter_value(CounterName::ComponentTimedOutRequestsTotal, protocol),
                0.0
            );
            assert_eq!(
                counter_value(CounterName::ComponentLoadShedRequestsTotal, protocol),
                0.0
            );
        }
    }

    #[expect(
        clippy::float_cmp,
        reason = "These deterministic fixture values are compared exactly."
    )]
    #[tokio::test]
    async fn cancelled_acknowledgement_waits_leave_admission_available() {
        init_metrics();
        let control = RequestControl::new(Some(1), None);
        let http = control
            .http_layer(http_error_response)
            .layer(http_acknowledging_service());
        let grpc = control
            .grpc_layer(grpc_error_response)
            .layer(grpc_acknowledging_service());

        let (http_event, receiver) = acknowledged_event();
        let mut http_wait = Box::pin(http.clone().oneshot(http_request_awaiting(receiver)));
        assert!(futures::poll!(&mut http_wait).is_pending());
        let (grpc_event, receiver) = acknowledged_event();
        let mut grpc_wait = Box::pin(grpc.clone().oneshot(grpc_request_awaiting(receiver)));
        assert!(futures::poll!(&mut grpc_wait).is_pending());

        // Clients disconnect while their deliveries are still pending.
        drop(http_wait);
        drop(grpc_wait);
        finalize(http_event, EventStatus::Errored);
        finalize(grpc_event, EventStatus::Rejected);

        assert_eq!(control.semaphore.as_ref().unwrap().available_permits(), 1);
        assert_eq!(active_requests(), 0.0);
        assert_eq!(
            http.oneshot(http_request()).await.unwrap().status(),
            StatusCode::OK
        );
        assert_eq!(
            grpc.oneshot(grpc_request()).await.unwrap().headers()["grpc-status"],
            "0"
        );
    }

    #[tokio::test]
    async fn grpc_acknowledgement_is_awaited_once_across_extension_clones() {
        let (event, receiver) = acknowledged_event();
        let mut response = grpc_ok();
        response
            .extensions_mut()
            .insert(SharedPendingAcknowledgement::new(
                PendingAcknowledgement::new(receiver, grpc_acknowledgement_failure),
            ));
        let cloned = response.extensions().clone();

        let mut finalizing = Box::pin(finalize_acknowledgement(response));
        assert!(futures::poll!(&mut finalizing).is_pending());
        // The awaiting response owns the receiver; the surviving clone cannot take it again.
        assert!(
            cloned
                .get::<SharedPendingAcknowledgement<GrpcResponse>>()
                .unwrap()
                .take()
                .is_none()
        );

        finalize(event, EventStatus::Rejected);
        assert_eq!(finalizing.await.unwrap().headers()["grpc-status"], "15");
    }
}
