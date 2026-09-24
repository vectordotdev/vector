use std::{convert::Infallible, sync::Arc, time::Duration};

use http::{Request, Response, StatusCode};
use hyper::Body;
use metrics::{Counter, Gauge};
use tokio::sync::Semaphore;
use tonic::body::BoxBody;
use tower::{
    BoxError, Layer, Service, ServiceExt, limit::GlobalConcurrencyLimitLayer,
    load_shed::error::Overloaded, service_fn, util::BoxCloneService,
};
use vector_lib::{
    counter,
    event::{BatchStatus, BatchStatusReceiver},
    gauge,
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
    timeout: Duration,
    metrics: Arc<RequestControlMetrics>,
}

impl RequestControl {
    pub(crate) fn new(outer_capacity: usize, inner_capacity: usize, timeout: Duration) -> Self {
        Self {
            outer: Arc::new(Semaphore::new(outer_capacity)),
            inner: Arc::new(Semaphore::new(inner_capacity)),
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
            timeout: self.timeout,
            metrics: Arc::clone(&self.metrics),
            error_response,
        }
    }
}

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

    fn queued_token(&self) -> OpenToken<impl Fn(usize) + use<>> {
        let gauge = self.queued_level.clone();
        self.queued
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

pub(crate) struct PendingAcknowledgement(pub(crate) BatchStatusReceiver);

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

#[derive(Clone)]
pub(crate) struct RequestControlLayer<R> {
    outer: Arc<Semaphore>,
    inner: Arc<Semaphore>,
    timeout: Duration,
    metrics: Arc<RequestControlMetrics>,
    error_response: R,
}

impl<S, R, B> Layer<S> for RequestControlLayer<R>
where
    S: Service<Request<Body>, Response = Response<B>> + Clone + Send + 'static,
    B: Send + 'static,
    S::Error: Into<BoxError> + Send + Sync + 'static,
    S::Future: Send + 'static,
    R: ErrorResponse<Response<B>>,
{
    type Service = BoxCloneService<Request<Body>, Response<B>, Infallible>;

    fn layer(&self, service: S) -> Self::Service {
        let processing =
            GlobalConcurrencyLimitLayer::with_semaphore(Arc::clone(&self.inner)).layer(service);

        let outer = Arc::clone(&self.outer);
        let metrics = Arc::clone(&self.metrics);
        let timeout = self.timeout;
        let error_response = self.error_response.clone();
        let service = service_fn(move |request: Request<Body>| {
            let deadline = tokio::time::Instant::now().checked_add(timeout);
            let outer = deadline.map(|_| Arc::clone(&outer).try_acquire_owned());
            let queued = outer
                .as_ref()
                .and_then(|outer| outer.as_ref().ok())
                .map(|_| metrics.queued_token());
            let processing = outer
                .as_ref()
                .and_then(|outer| outer.as_ref().ok())
                .map(|_| processing.clone());
            let error_response = error_response.clone();

            async move {
                let result = match (deadline, outer) {
                    (None, None) => {
                        Err(Box::new(tower::timeout::error::Elapsed::new()) as BoxError)
                    }
                    (Some(deadline), Some(Ok(outer))) => {
                        let acknowledgement_error = error_response.clone();
                        let admitted = async move {
                            let _outer = outer;
                            let mut processing =
                                processing.expect("admitted request has a processing service");

                            processing.ready().await.map_err(Into::into)?;
                            drop(queued);
                            let mut response =
                                processing.call(request).await.map_err(Into::into)?;

                            if let Some(PendingAcknowledgement(receiver)) =
                                response.extensions_mut().remove()
                                && let Some(status) =
                                    AcknowledgementFailure::from_status(receiver.await)
                            {
                                response =
                                    acknowledgement_error.make_acknowledgement_response(status);
                            }

                            Ok(response)
                        };

                        match tokio::time::timeout_at(deadline, admitted).await {
                            Ok(result) => result,
                            Err(_) => {
                                Err(Box::new(tower::timeout::error::Elapsed::new()) as BoxError)
                            }
                        }
                    }
                    // Reuse Tower's standard overload marker without its readiness-based
                    // layer, which would allow idle service clones to reserve capacity.
                    (Some(_), Some(Err(_))) => Err(Box::new(Overloaded::new()) as BoxError),
                    _ => unreachable!("outer admission is attempted only with a valid deadline"),
                };

                Ok::<_, Infallible>(match result {
                    Ok(response) => response,
                    Err(error) => error_response.make_response(error),
                })
            }
        });

        BoxCloneService::new(service)
    }
}

pub(crate) trait ErrorResponse<R>: Clone + Send + 'static {
    fn make_response(&self, error: BoxError) -> R;
    fn make_acknowledgement_response(&self, status: AcknowledgementFailure) -> R;
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

    fn make_acknowledgement_response(&self, status: AcknowledgementFailure) -> Response<Body> {
        let message = match status {
            AcknowledgementFailure::Errored => "Error delivering contents to sink",
            AcknowledgementFailure::Rejected => "Contents failed to deliver to sink",
        };
        let response = protobuf(Status {
            code: tonic::Code::Unknown as i32,
            message: message.to_owned(),
            ..Default::default()
        });
        warp::reply::with_status(response, StatusCode::INTERNAL_SERVER_ERROR).into_response()
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

    fn make_acknowledgement_response(&self, status: AcknowledgementFailure) -> Response<BoxBody> {
        match status {
            AcknowledgementFailure::Errored => tonic::Status::internal("Delivery error"),
            AcknowledgementFailure::Rejected => tonic::Status::data_loss("Delivery failed"),
        }
        .to_http()
    }
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
    use hyper::body::HttpBody;
    use prost::Message;
    use tokio::sync::Semaphore;
    use tower::{Layer, ServiceExt};
    use vector_lib::event::BatchNotifier;

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
                    .insert(PendingAcknowledgement(receiver));
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
        let service = control.http_layer().layer(acknowledging_http_service(
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
    async fn inner_error_releases_capacity() {
        let control = RequestControl::new(1, 1, Duration::from_secs(5));
        let service = control.http_layer().layer(FailingService);

        let response = service.oneshot(Request::new(Body::empty())).await.unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(control.outer.available_permits(), 1);
        assert_eq!(control.inner.available_permits(), 1);
        wait_for_level(&control.metrics.queued, 0).await;
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
    async fn canceled_queued_request_never_reaches_inner_service() {
        let control = RequestControl::new(2, 1, Duration::from_secs(5));
        let observations = Arc::new(Observations::default());
        let gate = Arc::new(Semaphore::new(0));
        let service = control
            .http_layer()
            .layer(http_service(Arc::clone(&observations), Arc::clone(&gate)));

        let active = tokio::spawn(service.clone().oneshot(Request::new(Body::empty())));
        wait_for(&observations.started, 1).await;
        let queued = tokio::spawn(service.clone().oneshot(Request::new(Body::empty())));
        while control.outer.available_permits() != 0 {
            tokio::task::yield_now().await;
        }
        wait_for_level(&control.metrics.queued, 1).await;
        queued.abort();
        assert!(queued.await.unwrap_err().is_cancelled());

        gate.add_permits(1);
        active.await.unwrap().unwrap();
        wait_for_level(&control.metrics.queued, 0).await;
        assert_eq!(observations.started.load(Ordering::Acquire), 1);
        assert_eq!(control.outer.available_permits(), 2);
        assert_eq!(control.inner.available_permits(), 1);

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
