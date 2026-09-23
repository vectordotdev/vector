use std::{
    convert::Infallible,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
    time::Duration,
};

use http::{Request, Response, StatusCode};
use hyper::Body;
use metrics::{Counter, Gauge};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio_util::sync::PollSemaphore;
use tonic::body::BoxBody;
use tower::{
    BoxError, Layer, Service, ServiceExt, buffer::BufferLayer, load_shed::error::Overloaded,
    timeout::TimeoutLayer, util::BoxCloneService,
};
use vector_lib::{
    counter, gauge,
    internal_event::{CounterName, GaugeName},
};
use warp::Reply;

use super::{reply::protobuf, status::Status};
use crate::internal_events::{OpenGauge, OpenToken};

/// Admission limits shared by all HTTP and gRPC requests handled by one OTLP source.
#[derive(Clone)]
pub(crate) struct RequestControl {
    outer: Arc<Semaphore>,
    inner: Arc<Semaphore>,
    outer_capacity: usize,
    timeout: Duration,
    metrics: Arc<RequestControlMetrics>,
}

impl RequestControl {
    pub(crate) fn new(outer_capacity: usize, inner_capacity: usize, timeout: Duration) -> Self {
        Self {
            outer: Arc::new(Semaphore::new(outer_capacity)),
            inner: Arc::new(Semaphore::new(inner_capacity)),
            outer_capacity,
            timeout,
            metrics: Arc::new(RequestControlMetrics::new(outer_capacity)),
        }
    }

    pub(crate) fn http_layer(&self) -> RequestControlLayer<HttpErrorResponse> {
        self.layer(HttpErrorResponse {
            metrics: Arc::clone(&self.metrics),
        })
    }

    pub(crate) fn grpc_layer(&self) -> RequestControlLayer<GrpcErrorResponse> {
        self.layer(GrpcErrorResponse {
            metrics: Arc::clone(&self.metrics),
        })
    }

    fn layer<R>(&self, error_response: R) -> RequestControlLayer<R> {
        RequestControlLayer {
            outer: Arc::clone(&self.outer),
            inner: Arc::clone(&self.inner),
            outer_capacity: self.outer_capacity,
            timeout: self.timeout,
            metrics: Arc::clone(&self.metrics),
            error_response,
        }
    }
}

type LevelEmitter = Box<dyn Fn(usize) + Send + Sync>;

#[derive(Clone)]
struct LevelToken(#[expect(dead_code)] Arc<OpenToken<LevelEmitter>>);

#[derive(Clone, Copy)]
enum Protocol {
    Http,
    Grpc,
}

struct RequestControlMetrics {
    queued: OpenGauge,
    queued_level: Gauge,
    #[expect(dead_code)]
    queue_capacity: Gauge,
    http_timed_out: Counter,
    grpc_timed_out: Counter,
}

impl RequestControlMetrics {
    #[expect(clippy::cast_precision_loss)]
    fn new(queue_capacity: usize) -> Self {
        let queue_capacity_gauge = gauge!(GaugeName::ComponentRequestQueueCapacity);
        queue_capacity_gauge.set(queue_capacity as f64);

        Self {
            queued: OpenGauge::new(),
            queued_level: gauge!(GaugeName::ComponentRequestQueueSize),
            queue_capacity: queue_capacity_gauge,
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

    fn queued_token(&self) -> LevelToken {
        let gauge = self.queued_level.clone();
        let emitter: LevelEmitter = Box::new(move |count| gauge.set(count as f64));
        LevelToken(Arc::new(self.queued.clone().open(emitter)))
    }

    fn time_out(&self, protocol: Protocol) {
        match protocol {
            Protocol::Http => &self.http_timed_out,
            Protocol::Grpc => &self.grpc_timed_out,
        }
        .increment(1);
    }
}

#[derive(Clone)]
struct QueuedRequest(#[expect(dead_code)] LevelToken);

#[derive(Clone)]
pub(crate) struct RequestProcessingPermit(Arc<Mutex<Option<OwnedSemaphorePermit>>>);

impl RequestProcessingPermit {
    fn new(permit: OwnedSemaphorePermit) -> Self {
        Self(Arc::new(Mutex::new(Some(permit))))
    }

    pub(crate) fn release(&self) {
        drop(
            self.0
                .lock()
                .expect("processing permit lock poisoned")
                .take(),
        );
    }
}

struct ProcessingLimitService<S> {
    inner: S,
    semaphore: Arc<Semaphore>,
    acquire: PollSemaphore,
    permit: Option<OwnedSemaphorePermit>,
}

impl<S> ProcessingLimitService<S> {
    fn new(inner: S, semaphore: Arc<Semaphore>) -> Self {
        Self {
            inner,
            acquire: PollSemaphore::new(Arc::clone(&semaphore)),
            semaphore,
            permit: None,
        }
    }
}

impl<S: Clone> Clone for ProcessingLimitService<S> {
    fn clone(&self) -> Self {
        Self::new(self.inner.clone(), Arc::clone(&self.semaphore))
    }
}

impl<S> Service<Request<Body>> for ProcessingLimitService<S>
where
    S: Service<Request<Body>>,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<S::Response, S::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        if self.permit.is_none() {
            match self.acquire.poll_acquire(cx) {
                Poll::Ready(Some(permit)) => self.permit = Some(permit),
                Poll::Ready(None) => unreachable!("processing semaphore is never closed"),
                Poll::Pending => return Poll::Pending,
            }
        }
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut request: Request<Body>) -> Self::Future {
        let permit = self
            .permit
            .take()
            .expect("processing limit must be ready before call");
        drop(request.extensions_mut().remove::<QueuedRequest>());
        let processing = RequestProcessingPermit::new(permit);
        request.extensions_mut().insert(processing.clone());
        let future = self.inner.call(request);
        Box::pin(async move {
            let _processing = processing;
            future.await
        })
    }
}

#[derive(Clone)]
struct AdmissionService<S> {
    inner: S,
    outer: Arc<Semaphore>,
    metrics: Arc<RequestControlMetrics>,
}

impl<S> Service<Request<Body>> for AdmissionService<S>
where
    S: Service<Request<Body>> + Clone + Send + 'static,
    S::Response: Send + 'static,
    S::Error: Into<BoxError>,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = BoxError;
    type Future = Pin<Box<dyn Future<Output = Result<S::Response, BoxError>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, mut request: Request<Body>) -> Self::Future {
        let outer = match Arc::clone(&self.outer).try_acquire_owned() {
            Ok(permit) => permit,
            Err(_) => {
                return Box::pin(async { Err(Box::new(Overloaded::new()) as BoxError) });
            }
        };

        request
            .extensions_mut()
            .insert(QueuedRequest(self.metrics.queued_token()));
        let future = self.inner.clone().oneshot(request);
        Box::pin(async move {
            let _outer = outer;
            future.await.map_err(Into::into)
        })
    }
}

#[derive(Clone)]
pub(crate) struct RequestControlLayer<R> {
    outer: Arc<Semaphore>,
    inner: Arc<Semaphore>,
    outer_capacity: usize,
    timeout: Duration,
    metrics: Arc<RequestControlMetrics>,
    error_response: R,
}

impl<S, R> Layer<S> for RequestControlLayer<R>
where
    S: Service<Request<Body>> + Clone + Send + 'static,
    S::Response: Send + 'static,
    S::Error: Into<BoxError> + Send + Sync,
    S::Future: Send + 'static,
    R: ErrorResponse<S::Response>,
{
    type Service = RequestControlService<S::Response, R>;

    fn layer(&self, service: S) -> Self::Service {
        let service = ProcessingLimitService::new(service, Arc::clone(&self.inner));
        let service = BufferLayer::new(self.outer_capacity).layer(service);
        let service = AdmissionService {
            inner: service,
            outer: Arc::clone(&self.outer),
            metrics: Arc::clone(&self.metrics),
        };
        let service = TimeoutLayer::new(self.timeout).layer(service);

        RequestControlService {
            inner: BoxCloneService::new(service),
            error_response: self.error_response.clone(),
        }
    }
}

pub(crate) trait ErrorResponse<R>: Clone + Send + 'static {
    fn make_response(&self, error: BoxError) -> R;
}

pub(crate) struct RequestControlService<R, E> {
    inner: BoxCloneService<Request<Body>, R, BoxError>,
    error_response: E,
}

impl<R, E> Clone for RequestControlService<R, E>
where
    E: Clone,
{
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            error_response: self.error_response.clone(),
        }
    }
}

impl<R, E> Service<Request<Body>> for RequestControlService<R, E>
where
    R: Send + 'static,
    E: ErrorResponse<R>,
{
    type Response = R;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<R, Infallible>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: Request<Body>) -> Self::Future {
        let error_response = self.error_response.clone();
        let future = self.inner.call(request);
        Box::pin(async move {
            Ok(match future.await {
                Ok(response) => response,
                Err(error) => error_response.make_response(error),
            })
        })
    }
}

#[derive(Clone)]
pub(crate) struct HttpErrorResponse {
    metrics: Arc<RequestControlMetrics>,
}

impl ErrorResponse<Response<Body>> for HttpErrorResponse {
    fn make_response(&self, error: BoxError) -> Response<Body> {
        let (status, message) = if error.is::<tower::load_shed::error::Overloaded>() {
            (StatusCode::TOO_MANY_REQUESTS, "OTLP request limit exceeded")
        } else if error.is::<tower::timeout::error::Elapsed>() {
            self.metrics.time_out(Protocol::Http);
            (StatusCode::SERVICE_UNAVAILABLE, "OTLP request timed out")
        } else {
            error!(message = "OTLP HTTP request middleware failed.", %error);
            (StatusCode::SERVICE_UNAVAILABLE, "OTLP request unavailable")
        };

        let response = protobuf(Status {
            code: tonic::Code::Unavailable as i32,
            message: message.to_owned(),
            ..Default::default()
        });
        warp::reply::with_status(response, status).into_response()
    }
}

#[derive(Clone)]
pub(crate) struct GrpcErrorResponse {
    metrics: Arc<RequestControlMetrics>,
}

impl ErrorResponse<Response<BoxBody>> for GrpcErrorResponse {
    fn make_response(&self, error: BoxError) -> Response<BoxBody> {
        let message = if error.is::<tower::load_shed::error::Overloaded>() {
            "OTLP request limit exceeded".to_owned()
        } else if error.is::<tower::timeout::error::Elapsed>() {
            self.metrics.time_out(Protocol::Grpc);
            "OTLP request timed out".to_owned()
        } else {
            error!(message = "OTLP gRPC request middleware failed.", %error);
            "OTLP request unavailable".to_owned()
        };

        tonic::Status::unavailable(message).to_http()
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        time::Duration,
    };

    use bytes::BytesMut;
    use futures_util::future::BoxFuture;
    use hyper::body::HttpBody;
    use prost::Message;
    use tokio::{sync::Semaphore, time::sleep};
    use tower::{Layer, ServiceExt};

    use super::*;

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
        release_processing: bool,
    }

    impl<R> Clone for GateService<R> {
        fn clone(&self) -> Self {
            Self {
                observations: Arc::clone(&self.observations),
                gate: Arc::clone(&self.gate),
                response: Arc::clone(&self.response),
                release_processing: self.release_processing,
            }
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

        fn call(&mut self, request: Request<Body>) -> Self::Future {
            let observations = Arc::clone(&self.observations);
            let gate = Arc::clone(&self.gate);
            let response = Arc::clone(&self.response);
            let processing = self.release_processing.then(|| {
                request
                    .extensions()
                    .get::<RequestProcessingPermit>()
                    .unwrap()
                    .clone()
            });
            Box::pin(async move {
                observations.started.fetch_add(1, Ordering::AcqRel);
                let active = observations.active.fetch_add(1, Ordering::AcqRel) + 1;
                observations
                    .maximum_active
                    .fetch_max(active, Ordering::AcqRel);
                if let Some(processing) = processing {
                    processing.release();
                }
                let _guard = ActiveGuard(observations);
                let _permit = gate.acquire().await.expect("test gate must remain open");
                Ok(response())
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
            release_processing: false,
        }
    }

    fn releasing_http_service(
        observations: Arc<Observations>,
        gate: Arc<Semaphore>,
    ) -> GateService<Response<Body>> {
        let mut service = http_service(observations, gate);
        service.release_processing = true;
        service
    }

    fn grpc_service(
        observations: Arc<Observations>,
        gate: Arc<Semaphore>,
    ) -> GateService<Response<BoxBody>> {
        GateService {
            observations,
            gate,
            response: Arc::new(|| tonic::Status::new(tonic::Code::Ok, "").to_http()),
            release_processing: false,
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
    async fn readiness_does_not_reserve_outer_capacity() {
        const OUTER: usize = 3;

        let control = RequestControl::new(OUTER, 1, Duration::from_secs(5));
        let observations = Arc::new(Observations::default());
        let gate = Arc::new(Semaphore::new(1));
        let service = control.http_layer().layer(http_service(observations, gate));
        let mut idle_services = vec![service.clone(); OUTER];

        for idle in &mut idle_services {
            idle.ready().await.unwrap();
        }

        assert_eq!(control.outer.available_permits(), OUTER);
        let response = service.oneshot(Request::new(Body::empty())).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn released_processing_permit_allows_ack_waits_to_overlap() {
        let control = RequestControl::new(2, 1, Duration::from_secs(5));
        let observations = Arc::new(Observations::default());
        let gate = Arc::new(Semaphore::new(0));
        let service = control.http_layer().layer(releasing_http_service(
            Arc::clone(&observations),
            Arc::clone(&gate),
        ));

        let first = tokio::spawn(service.clone().oneshot(Request::new(Body::empty())));
        wait_for(&observations.started, 1).await;
        let second = tokio::spawn(service.oneshot(Request::new(Body::empty())));
        wait_for(&observations.started, 2).await;
        assert_eq!(observations.maximum_active.load(Ordering::Acquire), 2);

        gate.add_permits(2);
        first.await.unwrap().unwrap();
        second.await.unwrap().unwrap();
        assert_eq!(control.inner.available_permits(), 1);
    }

    #[tokio::test]
    async fn queues_to_outer_limit_and_respects_inner_limit() {
        let control = RequestControl::new(3, 1, Duration::from_secs(5));
        let observations = Arc::new(Observations::default());
        let gate = Arc::new(Semaphore::new(0));
        let service = control
            .http_layer()
            .layer(http_service(Arc::clone(&observations), Arc::clone(&gate)));

        let first = tokio::spawn(service.clone().oneshot(Request::new(Body::empty())));
        wait_for(&observations.started, 1).await;
        let second = tokio::spawn(service.clone().oneshot(Request::new(Body::empty())));
        let third = tokio::spawn(service.clone().oneshot(Request::new(Body::empty())));
        while control.outer.available_permits() != 0 {
            tokio::task::yield_now().await;
        }
        wait_for_level(&control.metrics.queued, 2).await;

        let overloaded = service
            .clone()
            .oneshot(Request::new(Body::empty()))
            .await
            .unwrap();
        assert_eq!(overloaded.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(observations.maximum_active.load(Ordering::Acquire), 1);

        gate.add_permits(3);
        for request in [first, second, third] {
            assert_eq!(request.await.unwrap().unwrap().status(), StatusCode::OK);
        }
        assert_eq!(observations.started.load(Ordering::Acquire), 3);
        assert_eq!(observations.maximum_active.load(Ordering::Acquire), 1);
        wait_for_level(&control.metrics.queued, 0).await;
    }

    #[tokio::test]
    async fn synchronized_burst_is_bounded_by_both_stages() {
        const INNER: usize = 16;
        const OUTER: usize = 160;

        let control = RequestControl::new(OUTER, INNER, Duration::from_secs(5));
        let observations = Arc::new(Observations::default());
        let gate = Arc::new(Semaphore::new(0));
        let service = control
            .http_layer()
            .layer(http_service(Arc::clone(&observations), Arc::clone(&gate)));

        let mut admitted = Vec::with_capacity(OUTER);
        for _ in 0..OUTER {
            admitted.push(tokio::spawn(
                service.clone().oneshot(Request::new(Body::empty())),
            ));
        }
        while control.outer.available_permits() != 0
            || observations.started.load(Ordering::Acquire) != INNER
        {
            tokio::task::yield_now().await;
        }
        wait_for_level(&control.metrics.queued, OUTER - INNER).await;

        let overloaded = service
            .clone()
            .oneshot(Request::new(Body::empty()))
            .await
            .unwrap();
        assert_eq!(overloaded.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(observations.maximum_active.load(Ordering::Acquire), INNER);

        gate.add_permits(OUTER);
        for request in admitted {
            assert_eq!(request.await.unwrap().unwrap().status(), StatusCode::OK);
        }
        assert_eq!(observations.started.load(Ordering::Acquire), OUTER);
        assert_eq!(observations.maximum_active.load(Ordering::Acquire), INNER);
        wait_for_level(&control.metrics.queued, 0).await;
    }

    #[tokio::test]
    async fn timeout_and_cancellation_release_capacity() {
        let control = RequestControl::new(1, 1, Duration::from_millis(20));
        let observations = Arc::new(Observations::default());
        let gate = Arc::new(Semaphore::new(0));
        let service = control
            .http_layer()
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
        assert_eq!(control.outer.available_permits(), 1);
        assert_eq!(control.inner.available_permits(), 1);
        wait_for_level(&control.metrics.queued, 0).await;

        let pending = tokio::spawn(service.clone().oneshot(Request::new(Body::empty())));
        wait_for(&observations.started, 2).await;
        pending.abort();
        assert!(pending.await.unwrap_err().is_cancelled());
        wait_for(&observations.active, 0).await;
        assert_eq!(control.outer.available_permits(), 1);
        assert_eq!(control.inner.available_permits(), 1);
        wait_for_level(&control.metrics.queued, 0).await;
    }

    #[tokio::test]
    async fn canceled_buffered_request_never_reaches_inner_service() {
        let control = RequestControl::new(2, 1, Duration::from_secs(5));
        let observations = Arc::new(Observations::default());
        let gate = Arc::new(Semaphore::new(0));
        let service = control
            .http_layer()
            .layer(http_service(Arc::clone(&observations), Arc::clone(&gate)));

        let active = tokio::spawn(service.clone().oneshot(Request::new(Body::empty())));
        wait_for(&observations.started, 1).await;
        let buffered = tokio::spawn(service.clone().oneshot(Request::new(Body::empty())));
        while control.outer.available_permits() != 0 {
            tokio::task::yield_now().await;
        }
        wait_for_level(&control.metrics.queued, 1).await;
        buffered.abort();
        assert!(buffered.await.unwrap_err().is_cancelled());

        gate.add_permits(1);
        active.await.unwrap().unwrap();
        wait_for_level(&control.metrics.queued, 0).await;
        sleep(Duration::from_millis(20)).await;
        assert_eq!(observations.started.load(Ordering::Acquire), 1);
        assert_eq!(control.outer.available_permits(), 2);

        // The inner concurrency service may retain a readiness reservation for the next call;
        // prove that it remains usable rather than inspecting its semaphore directly.
        gate.add_permits(1);
        service.oneshot(Request::new(Body::empty())).await.unwrap();
        assert_eq!(observations.started.load(Ordering::Acquire), 2);
        assert_eq!(control.inner.available_permits(), 1);
    }

    #[tokio::test]
    async fn http_and_grpc_share_outer_and_inner_capacity() {
        let control = RequestControl::new(2, 1, Duration::from_secs(5));
        let http_observations = Arc::new(Observations::default());
        let grpc_observations = Arc::new(Observations::default());
        let gate = Arc::new(Semaphore::new(0));
        let http = control.http_layer().layer(http_service(
            Arc::clone(&http_observations),
            Arc::clone(&gate),
        ));
        let grpc = control.grpc_layer().layer(grpc_service(
            Arc::clone(&grpc_observations),
            Arc::clone(&gate),
        ));

        let processing = tokio::spawn(http.clone().oneshot(Request::new(Body::empty())));
        wait_for(&http_observations.started, 1).await;
        let queued = tokio::spawn(grpc.clone().oneshot(Request::new(Body::empty())));
        while control.outer.available_permits() != 0 {
            tokio::task::yield_now().await;
        }
        wait_for_level(&control.metrics.queued, 1).await;
        assert_eq!(grpc_observations.started.load(Ordering::Acquire), 0);

        let overloaded = grpc
            .clone()
            .oneshot(Request::new(Body::empty()))
            .await
            .unwrap();
        assert_eq!(overloaded.headers()["grpc-status"], "14");

        gate.add_permits(2);
        processing.await.unwrap().unwrap();
        queued.await.unwrap().unwrap();
        assert_eq!(grpc_observations.started.load(Ordering::Acquire), 1);
        wait_for_level(&control.metrics.queued, 0).await;
    }
}
