use std::{
    convert::Infallible,
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};

use http::{Request, Response, StatusCode};
use hyper::Body;
use tokio::sync::Semaphore;
use tonic::body::BoxBody;
use tower::{
    BoxError, Layer, Service, ServiceBuilder, buffer::BufferLayer,
    limit::GlobalConcurrencyLimitLayer, load_shed::LoadShedLayer, timeout::TimeoutLayer,
    util::BoxCloneService,
};
use warp::Reply;

use super::{reply::protobuf, status::Status};

/// Admission limits shared by all HTTP and gRPC requests handled by one OTLP source.
#[derive(Clone, Debug)]
pub(crate) struct RequestControl {
    outer: Arc<Semaphore>,
    inner: Arc<Semaphore>,
    outer_capacity: usize,
    timeout: Duration,
}

impl RequestControl {
    pub(crate) fn new(outer_capacity: usize, inner_capacity: usize, timeout: Duration) -> Self {
        Self {
            outer: Arc::new(Semaphore::new(outer_capacity)),
            inner: Arc::new(Semaphore::new(inner_capacity)),
            outer_capacity,
            timeout,
        }
    }

    pub(crate) fn http_layer(&self) -> RequestControlLayer<HttpErrorResponse> {
        self.layer(HttpErrorResponse)
    }

    pub(crate) fn grpc_layer(&self) -> RequestControlLayer<GrpcErrorResponse> {
        self.layer(GrpcErrorResponse)
    }

    fn layer<R>(&self, error_response: R) -> RequestControlLayer<R> {
        RequestControlLayer {
            outer: Arc::clone(&self.outer),
            inner: Arc::clone(&self.inner),
            outer_capacity: self.outer_capacity,
            timeout: self.timeout,
            error_response,
        }
    }
}

#[derive(Clone)]
pub(crate) struct RequestControlLayer<R> {
    outer: Arc<Semaphore>,
    inner: Arc<Semaphore>,
    outer_capacity: usize,
    timeout: Duration,
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
        // Layer order is outer-to-inner. Load shedding observes readiness from the outer
        // semaphore, while the buffer waits on the inner semaphore without polling bodies.
        let service = ServiceBuilder::new()
            .layer(TimeoutLayer::new(self.timeout))
            .layer(LoadShedLayer::new())
            .layer(GlobalConcurrencyLimitLayer::with_semaphore(Arc::clone(
                &self.outer,
            )))
            .layer(BufferLayer::new(self.outer_capacity))
            .layer(GlobalConcurrencyLimitLayer::with_semaphore(Arc::clone(
                &self.inner,
            )))
            .service(service);

        RequestControlService {
            inner: BoxCloneService::new(service),
            readiness_error: None,
            error_response: self.error_response.clone(),
        }
    }
}

pub(crate) trait ErrorResponse<R>: Clone + Send + 'static {
    fn make_response(&self, error: BoxError) -> R;
}

pub(crate) struct RequestControlService<R, E> {
    inner: BoxCloneService<Request<Body>, R, BoxError>,
    readiness_error: Option<BoxError>,
    error_response: E,
}

impl<R, E> Clone for RequestControlService<R, E>
where
    E: Clone,
{
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            // Readiness reservations are local to a service clone and must not be copied.
            readiness_error: None,
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

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        match self.inner.poll_ready(cx) {
            Poll::Ready(Ok(())) => Poll::Ready(Ok(())),
            Poll::Ready(Err(error)) => {
                self.readiness_error = Some(error);
                Poll::Ready(Ok(()))
            }
            Poll::Pending => Poll::Pending,
        }
    }

    fn call(&mut self, request: Request<Body>) -> Self::Future {
        let error_response = self.error_response.clone();
        if let Some(error) = self.readiness_error.take() {
            return Box::pin(async move { Ok(error_response.make_response(error)) });
        }

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
pub(crate) struct HttpErrorResponse;

impl ErrorResponse<Response<Body>> for HttpErrorResponse {
    fn make_response(&self, error: BoxError) -> Response<Body> {
        let (status, message) = if error.is::<tower::load_shed::error::Overloaded>() {
            (StatusCode::TOO_MANY_REQUESTS, "OTLP request limit exceeded")
        } else if error.is::<tower::timeout::error::Elapsed>() {
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
pub(crate) struct GrpcErrorResponse;

impl ErrorResponse<Response<BoxBody>> for GrpcErrorResponse {
    fn make_response(&self, error: BoxError) -> Response<BoxBody> {
        let message = if error.is::<tower::load_shed::error::Overloaded>() {
            "OTLP request limit exceeded".to_owned()
        } else if error.is::<tower::timeout::error::Elapsed>() {
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

        let pending = tokio::spawn(service.clone().oneshot(Request::new(Body::empty())));
        wait_for(&observations.started, 2).await;
        pending.abort();
        assert!(pending.await.unwrap_err().is_cancelled());
        wait_for(&observations.active, 0).await;
        assert_eq!(control.outer.available_permits(), 1);
        assert_eq!(control.inner.available_permits(), 1);
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
        buffered.abort();
        assert!(buffered.await.unwrap_err().is_cancelled());

        gate.add_permits(1);
        active.await.unwrap().unwrap();
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
    }
}
