use std::{convert::Infallible, sync::Arc, time::Duration};

use crate::internal_events::{OpenGauge, OpenToken};
use http::{Request, Response};
use hyper::Body;
use metrics::{Counter, Gauge};
use tokio::sync::Semaphore;
use tower::{
    BoxError, Layer, Service, ServiceExt, service_fn, timeout::TimeoutLayer, util::BoxCloneService,
};
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
        self.layer(error_response, Protocol::Http)
    }

    pub(crate) fn grpc_layer<R>(&self, error_response: R) -> RequestControlLayer<R> {
        self.layer(error_response, Protocol::Grpc)
    }

    fn layer<R>(&self, error_response: R, protocol: Protocol) -> RequestControlLayer<R> {
        RequestControlLayer {
            semaphore: Arc::clone(&self.semaphore),
            timeout: self.timeout,
            metrics: Arc::clone(&self.metrics),
            protocol,
            error_response,
        }
    }
}

#[derive(Clone, Copy)]
enum Protocol {
    Http,
    Grpc,
}

#[derive(Clone, Copy)]
pub(crate) enum MiddlewareError {
    Overloaded,
    TimedOut,
    Unavailable,
}

impl MiddlewareError {
    pub(crate) const fn message(self) -> &'static str {
        match self {
            Self::Overloaded => "OTLP request limit exceeded",
            Self::TimedOut => "OTLP request timed out",
            Self::Unavailable => "OTLP request unavailable",
        }
    }
}

struct RequestControlMetrics {
    active: OpenGauge,
    active_level: Gauge,
    #[expect(
        dead_code,
        reason = "retain the concurrency limit gauge handle for the controller lifetime"
    )]
    concurrency_limit: Gauge,
    http_timed_out: Counter,
    grpc_timed_out: Counter,
}

impl RequestControlMetrics {
    #[expect(clippy::cast_precision_loss)]
    fn new(concurrency_limit: usize) -> Self {
        let concurrency_limit_gauge = gauge!(GaugeName::ComponentRequestConcurrencyLimit);
        concurrency_limit_gauge.set(concurrency_limit as f64);

        Self {
            active: OpenGauge::new(),
            active_level: gauge!(GaugeName::ComponentRequestActive),
            concurrency_limit: concurrency_limit_gauge,
            http_timed_out: counter!(
                CounterName::ComponentTimedOutRequestsTotal,
                "protocol" => "http"
            ),
            grpc_timed_out: counter!(
                CounterName::ComponentTimedOutRequestsTotal,
                "protocol" => "grpc"
            ),
        }
    }

    fn active_token(&self) -> OpenToken<impl Fn(usize) + use<>> {
        let gauge = self.active_level.clone();
        self.active
            .clone()
            .open(move |count| gauge.set(count as f64))
    }

    fn time_out(&self, protocol: Protocol) {
        match protocol {
            Protocol::Http => &self.http_timed_out,
            Protocol::Grpc => &self.grpc_timed_out,
        }
        .increment(1);
    }
}

fn classify_error(
    error: BoxError,
    metrics: &RequestControlMetrics,
    protocol: Protocol,
) -> MiddlewareError {
    if error.is::<tower::timeout::error::Elapsed>() {
        metrics.time_out(protocol);
        MiddlewareError::TimedOut
    } else {
        error!(message = "OTLP request middleware failed.", %error);
        MiddlewareError::Unavailable
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
    semaphore: Arc<Semaphore>,
    timeout: Duration,
    metrics: Arc<RequestControlMetrics>,
    protocol: Protocol,
    error_response: R,
}

impl<S, R, B> Layer<S> for RequestControlLayer<R>
where
    S: Service<Request<Body>, Response = Response<B>> + Clone + Send + 'static,
    B: Send + 'static,
    S::Error: Into<BoxError> + Send + Sync + 'static,
    S::Future: Send + 'static,
    R: MiddlewareErrorResponse<Response<B>>,
{
    type Service = BoxCloneService<Request<Body>, Response<B>, Infallible>;

    fn layer(&self, service: S) -> Self::Service {
        // Acquire shared capacity immediately, reject requests when none is available, and release
        // the permit before finalizing the acknowledgement without a timeout.
        let processing = TimeoutLayer::new(self.timeout).layer(service);
        let timeout = self.timeout;
        let semaphore = Arc::clone(&self.semaphore);
        let metrics = Arc::clone(&self.metrics);
        let protocol = self.protocol;
        let error_response = self.error_response.clone();
        let service = service_fn(move |request: Request<Body>| {
            let admitted = tokio::time::Instant::now().checked_add(timeout).map(|_| {
                Arc::clone(&semaphore).try_acquire_owned().map(|permit| {
                    let active = metrics.active_token();
                    (permit, active, processing.clone())
                })
            });
            let metrics = Arc::clone(&metrics);
            let error_response = error_response.clone();

            async move {
                let response = match admitted {
                    Some(Ok((_permit, _active, processing))) => {
                        match processing.oneshot(request).await {
                            Ok(response) => response,
                            Err(error) => error_response
                                .make_response(classify_error(error, &metrics, protocol)),
                        }
                    }
                    Some(Err(_)) => error_response.make_response(MiddlewareError::Overloaded),
                    None => {
                        metrics.time_out(protocol);
                        error_response.make_response(MiddlewareError::TimedOut)
                    }
                };

                Ok::<_, Infallible>(response)
            }
        });
        let service = service.and_then(finalize_acknowledgement);

        BoxCloneService::new(service)
    }
}

pub(crate) trait MiddlewareErrorResponse<R>: Clone + Send + 'static {
    fn make_response(&self, error: MiddlewareError) -> R;
}

#[cfg(test)]
mod tests {
    use std::{
        future::{Ready, ready},
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        task::{Context, Poll},
        time::Duration,
    };

    use bytes::BytesMut;
    use futures_util::future::BoxFuture;
    use http::StatusCode;
    use hyper::body::HttpBody;
    use prost::Message;
    use tokio::sync::Semaphore;
    use tonic::body::BoxBody;
    use tower::{Layer, ServiceExt};
    use vector_lib::event::BatchNotifier;

    use super::*;
    use crate::sources::opentelemetry::{
        grpc::GrpcErrorResponse, http::HttpErrorResponse, status::Status,
    };

    #[derive(Default)]
    struct Observations {
        started: AtomicUsize,
        active: AtomicUsize,
        maximum_active: AtomicUsize,
    }

    struct GateService<R> {
        observations: Arc<Observations>,
        gate: Arc<Semaphore>,
        response: Arc<dyn Fn() -> R + Send + Sync>,
    }

    #[derive(Clone)]
    struct AcknowledgingGateService {
        observations: Arc<Observations>,
        gate: Arc<Semaphore>,
    }

    impl<R> Clone for GateService<R> {
        fn clone(&self) -> Self {
            Self {
                observations: Arc::clone(&self.observations),
                gate: Arc::clone(&self.gate),
                response: Arc::clone(&self.response),
            }
        }
    }

    #[derive(Clone)]
    struct FailingService;

    impl Service<Request<Body>> for FailingService {
        type Response = Response<Body>;
        type Error = std::io::Error;
        type Future = Ready<Result<Self::Response, Self::Error>>;

        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, _request: Request<Body>) -> Self::Future {
            ready(Err(std::io::Error::other("request failed")))
        }
    }

    struct ActiveGuard(Arc<Observations>);

    impl Drop for ActiveGuard {
        fn drop(&mut self) {
            self.0.active.fetch_sub(1, Ordering::AcqRel);
        }
    }

    impl<R> Service<Request<Body>> for GateService<R>
    where
        R: Send + 'static,
    {
        type Response = R;
        type Error = Infallible;
        type Future = BoxFuture<'static, Result<R, Infallible>>;

        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, _request: Request<Body>) -> Self::Future {
            let observations = Arc::clone(&self.observations);
            let gate = Arc::clone(&self.gate);
            let response = Arc::clone(&self.response);
            Box::pin(async move {
                observations.started.fetch_add(1, Ordering::AcqRel);
                let active = observations.active.fetch_add(1, Ordering::AcqRel) + 1;
                observations
                    .maximum_active
                    .fetch_max(active, Ordering::AcqRel);
                let _guard = ActiveGuard(observations);
                let _permit = gate.acquire().await.expect("test gate must remain open");
                Ok(response())
            })
        }
    }

    impl Service<Request<Body>> for AcknowledgingGateService {
        type Response = Response<Body>;
        type Error = Infallible;
        type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, _request: Request<Body>) -> Self::Future {
            let observations = Arc::clone(&self.observations);
            let gate = Arc::clone(&self.gate);
            Box::pin(async move {
                observations.started.fetch_add(1, Ordering::AcqRel);
                let active = observations.active.fetch_add(1, Ordering::AcqRel) + 1;
                observations
                    .maximum_active
                    .fetch_max(active, Ordering::AcqRel);

                let (notifier, receiver) = BatchNotifier::new_with_receiver();
                tokio::spawn(async move {
                    let _guard = ActiveGuard(observations);
                    let _permit = gate.acquire().await.expect("test gate must remain open");
                    drop(notifier);
                });

                let mut response = Response::new(Body::empty());
                response
                    .extensions_mut()
                    .insert(PendingAcknowledgement::new(receiver, |_| {
                        Response::new(Body::empty())
                    }));
                Ok(response)
            })
        }
    }

    fn http_service(
        observations: Arc<Observations>,
        gate: Arc<Semaphore>,
    ) -> GateService<Response<Body>> {
        GateService {
            observations,
            gate,
            response: Arc::new(|| Response::new(Body::empty())),
        }
    }

    fn acknowledging_http_service(
        observations: Arc<Observations>,
        gate: Arc<Semaphore>,
    ) -> AcknowledgingGateService {
        AcknowledgingGateService { observations, gate }
    }

    fn grpc_service(
        observations: Arc<Observations>,
        gate: Arc<Semaphore>,
    ) -> GateService<Response<BoxBody>> {
        GateService {
            observations,
            gate,
            response: Arc::new(|| tonic::Status::new(tonic::Code::Ok, "").to_http()),
        }
    }

    async fn wait_for(value: &AtomicUsize, expected: usize) {
        tokio::time::timeout(Duration::from_secs(1), async {
            while value.load(Ordering::Acquire) != expected {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("observation did not reach expected value");
    }

    async fn wait_for_level(level: &OpenGauge, expected: usize) {
        tokio::time::timeout(Duration::from_secs(1), async {
            while level.current() != expected {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("request control metric did not reach expected value");
    }

    #[tokio::test]
    async fn readiness_does_not_reserve_capacity() {
        const LIMIT: usize = 3;

        let control = RequestControl::new(LIMIT, Duration::from_secs(5));
        let observations = Arc::new(Observations::default());
        let gate = Arc::new(Semaphore::new(1));
        let service = control
            .http_layer(HttpErrorResponse)
            .layer(http_service(observations, gate));
        let mut idle_services = vec![service.clone(); LIMIT];

        for idle in &mut idle_services {
            idle.ready().await.unwrap();
        }

        assert_eq!(control.semaphore.available_permits(), LIMIT);
        let response = service.oneshot(Request::new(Body::empty())).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn released_request_permits_allow_ack_waits_to_overlap() {
        let control = RequestControl::new(1, Duration::from_secs(5));
        let observations = Arc::new(Observations::default());
        let gate = Arc::new(Semaphore::new(0));
        let service = control
            .http_layer(HttpErrorResponse)
            .layer(acknowledging_http_service(
                Arc::clone(&observations),
                Arc::clone(&gate),
            ));

        let first = tokio::spawn(service.clone().oneshot(Request::new(Body::empty())));
        wait_for(&observations.started, 1).await;
        let second = tokio::spawn(service.oneshot(Request::new(Body::empty())));
        wait_for(&observations.started, 2).await;
        assert_eq!(observations.maximum_active.load(Ordering::Acquire), 2);
        assert_eq!(control.semaphore.available_permits(), 1);

        gate.add_permits(2);
        first.await.unwrap().unwrap();
        second.await.unwrap().unwrap();
        assert_eq!(control.semaphore.available_permits(), 1);
    }

    #[tokio::test]
    async fn processing_error_releases_capacity() {
        let control = RequestControl::new(1, Duration::from_secs(5));
        let service = control.http_layer(HttpErrorResponse).layer(FailingService);

        let response = service.oneshot(Request::new(Body::empty())).await.unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(control.semaphore.available_permits(), 1);
        wait_for_level(&control.metrics.active, 0).await;
    }

    #[tokio::test]
    async fn rejects_requests_above_concurrency_limit() {
        let control = RequestControl::new(1, Duration::from_secs(5));
        let observations = Arc::new(Observations::default());
        let gate = Arc::new(Semaphore::new(0));
        let service = control
            .http_layer(HttpErrorResponse)
            .layer(http_service(Arc::clone(&observations), Arc::clone(&gate)));

        let admitted = tokio::spawn(service.clone().oneshot(Request::new(Body::empty())));
        wait_for(&observations.started, 1).await;
        assert_eq!(control.metrics.active.current(), 1);

        let rejected = service
            .clone()
            .oneshot(Request::new(Body::empty()))
            .await
            .unwrap();
        assert_eq!(rejected.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(observations.started.load(Ordering::Acquire), 1);

        gate.add_permits(1);
        assert_eq!(admitted.await.unwrap().unwrap().status(), StatusCode::OK);
        wait_for_level(&control.metrics.active, 0).await;
        assert_eq!(control.semaphore.available_permits(), 1);
    }

    #[tokio::test]
    async fn synchronized_burst_is_bounded_by_concurrency_limit() {
        const LIMIT: usize = 16;
        const REQUESTS: usize = 160;

        let control = RequestControl::new(LIMIT, Duration::from_secs(5));
        let observations = Arc::new(Observations::default());
        let gate = Arc::new(Semaphore::new(0));
        let service = control
            .http_layer(HttpErrorResponse)
            .layer(http_service(Arc::clone(&observations), Arc::clone(&gate)));

        let mut admitted = Vec::with_capacity(LIMIT);
        for _ in 0..LIMIT {
            admitted.push(tokio::spawn(
                service.clone().oneshot(Request::new(Body::empty())),
            ));
        }
        wait_for(&observations.started, LIMIT).await;
        assert_eq!(control.metrics.active.current(), LIMIT);

        for _ in LIMIT..REQUESTS {
            let response = service
                .clone()
                .oneshot(Request::new(Body::empty()))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        }
        assert_eq!(observations.started.load(Ordering::Acquire), LIMIT);
        assert_eq!(observations.maximum_active.load(Ordering::Acquire), LIMIT);

        gate.add_permits(LIMIT);
        for request in admitted {
            assert_eq!(request.await.unwrap().unwrap().status(), StatusCode::OK);
        }
        wait_for_level(&control.metrics.active, 0).await;
        assert_eq!(control.semaphore.available_permits(), LIMIT);
    }

    #[tokio::test]
    async fn timeout_and_cancellation_release_capacity() {
        let control = RequestControl::new(1, Duration::from_millis(20));
        let observations = Arc::new(Observations::default());
        let gate = Arc::new(Semaphore::new(0));
        let service = control
            .http_layer(HttpErrorResponse)
            .layer(http_service(Arc::clone(&observations), Arc::clone(&gate)));

        let timed_out = service
            .clone()
            .oneshot(Request::new(Body::empty()))
            .await
            .unwrap();
        assert_eq!(timed_out.status(), StatusCode::SERVICE_UNAVAILABLE);
        let mut body = timed_out.into_body();
        let mut bytes = BytesMut::new();
        while let Some(chunk) = body.data().await {
            bytes.extend_from_slice(&chunk.unwrap());
        }
        let status = Status::decode(bytes.freeze()).unwrap();
        assert_eq!(status.code, tonic::Code::Unavailable as i32);
        assert_eq!(control.semaphore.available_permits(), 1);
        wait_for_level(&control.metrics.active, 0).await;

        let pending = tokio::spawn(service.clone().oneshot(Request::new(Body::empty())));
        wait_for(&observations.started, 2).await;
        pending.abort();
        assert!(pending.await.unwrap_err().is_cancelled());
        wait_for(&observations.active, 0).await;
        assert_eq!(control.semaphore.available_permits(), 1);
        wait_for_level(&control.metrics.active, 0).await;
    }

    #[tokio::test]
    async fn http_and_grpc_share_capacity() {
        let control = RequestControl::new(1, Duration::from_secs(5));
        let http_observations = Arc::new(Observations::default());
        let grpc_observations = Arc::new(Observations::default());
        let gate = Arc::new(Semaphore::new(0));
        let http = control.http_layer(HttpErrorResponse).layer(http_service(
            Arc::clone(&http_observations),
            Arc::clone(&gate),
        ));
        let grpc = control.grpc_layer(GrpcErrorResponse).layer(grpc_service(
            Arc::clone(&grpc_observations),
            Arc::clone(&gate),
        ));

        let processing = tokio::spawn(http.clone().oneshot(Request::new(Body::empty())));
        wait_for(&http_observations.started, 1).await;
        let rejected = grpc
            .clone()
            .oneshot(Request::new(Body::empty()))
            .await
            .unwrap();
        assert_eq!(rejected.headers()["grpc-status"], "14");
        assert_eq!(grpc_observations.started.load(Ordering::Acquire), 0);

        gate.add_permits(1);
        processing.await.unwrap().unwrap();
        wait_for_level(&control.metrics.active, 0).await;
        assert_eq!(control.semaphore.available_permits(), 1);
    }
}
