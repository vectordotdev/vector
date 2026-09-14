use std::{
    convert::Infallible,
    net::SocketAddr,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll},
    time::Duration,
};

use futures::{FutureExt, StreamExt, future::BoxFuture};
use http::{HeaderMap, Request, Response};
use hyper::{Body, body::HttpBody};
use pin_project::pin_project;
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    net::TcpStream,
    time::{Sleep, sleep},
};
use tonic::{
    body::BoxBody,
    server::NamedService,
    transport::server::{Connected, Routes, Server},
};
use tower::{Layer, Service};
use tower_http::{
    classify::{GrpcErrorsAsFailures, SharedClassifier},
    trace::TraceLayer,
};
use tracing::Span;

use crate::{
    internal_events::{GrpcServerRequestReceived, GrpcServerResponseSent},
    shutdown::{ShutdownSignal, ShutdownSignalToken},
    tls::{MaybeTlsIncomingStream, MaybeTlsSettings, TlsAcceptorReloader},
};
use vector_lib::configurable::configurable_component;

mod decompression;
pub use self::decompression::{DecompressionAndMetrics, DecompressionAndMetricsLayer};

#[cfg(test)]
static MAX_CONNECTION_AGE_CONNECTION_OBSERVATIONS: std::sync::Mutex<Vec<SocketAddr>> =
    std::sync::Mutex::new(Vec::new());

#[cfg(all(test, feature = "sources-vector", feature = "sinks-vector"))]
pub(crate) mod test_support {
    use std::net::SocketAddr;

    use super::MAX_CONNECTION_AGE_CONNECTION_OBSERVATIONS;

    pub(crate) fn reset_max_connection_age_connection_observations() {
        MAX_CONNECTION_AGE_CONNECTION_OBSERVATIONS
            .lock()
            .unwrap()
            .clear();
    }

    pub(crate) fn max_connection_age_connection_observations() -> Vec<SocketAddr> {
        MAX_CONNECTION_AGE_CONNECTION_OBSERVATIONS
            .lock()
            .unwrap()
            .clone()
    }
}

/// Configuration of gRPC server keepalive parameters.
#[configurable_component]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GrpcKeepaliveConfig {
    /// The maximum amount of time a connection may exist before the server closes it.
    ///
    /// When unset, connections are not closed based on age.
    #[serde(default)]
    #[configurable(metadata(docs::examples = 300))]
    #[configurable(metadata(docs::type_unit = "seconds"))]
    #[configurable(metadata(docs::human_name = "Maximum Connection Age"))]
    pub max_connection_age_secs: Option<u64>,

    /// The grace period added to `max_connection_age_secs` before the server closes the connection.
    ///
    /// This setting only applies when `max_connection_age_secs` is set.
    #[serde(default)]
    #[configurable(metadata(docs::examples = 30))]
    #[configurable(metadata(docs::type_unit = "seconds"))]
    #[configurable(metadata(docs::human_name = "Maximum Connection Age Grace"))]
    pub max_connection_age_grace_secs: Option<u64>,
}

impl GrpcKeepaliveConfig {
    fn max_connection_lifetime(&self) -> Option<Duration> {
        self.max_connection_age_secs.map(|max_connection_age_secs| {
            let age = Duration::from_secs(max_connection_age_secs);
            let grace = self
                .max_connection_age_grace_secs
                .map(Duration::from_secs)
                .unwrap_or_default();

            age.checked_add(grace).unwrap_or(Duration::MAX)
        })
    }
}

struct MaxConnectionAgeIo {
    inner: MaybeTlsIncomingStream<TcpStream>,
    state: MaxConnectionAgeState,
}

impl MaxConnectionAgeIo {
    fn new(inner: MaybeTlsIncomingStream<TcpStream>, lifetime: Option<Duration>) -> Self {
        #[cfg(test)]
        if lifetime.is_some() {
            MAX_CONNECTION_AGE_CONNECTION_OBSERVATIONS
                .lock()
                .unwrap()
                .push(inner.peer_addr());
        }

        Self {
            inner,
            state: MaxConnectionAgeState::new(lifetime),
        }
    }
}

struct MaxConnectionAgeState {
    deadline: Option<Pin<Box<Sleep>>>,
    read_expired: bool,
    active_requests: Arc<AtomicUsize>,
}

impl MaxConnectionAgeState {
    fn new(lifetime: Option<Duration>) -> Self {
        Self {
            deadline: lifetime.map(|lifetime| Box::pin(sleep(lifetime))),
            read_expired: false,
            active_requests: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn is_read_expired(&mut self, cx: &mut Context<'_>) -> bool {
        if self.read_expired {
            return true;
        }

        self.read_expired = self
            .deadline
            .as_mut()
            .is_some_and(|deadline| deadline.as_mut().poll(cx).is_ready());

        self.read_expired
    }

    fn is_write_expired(&mut self, cx: &mut Context<'_>) -> bool {
        self.is_read_expired(cx) && self.active_requests.load(Ordering::Acquire) == 0
    }

    fn active_requests(&self) -> Arc<AtomicUsize> {
        Arc::clone(&self.active_requests)
    }

    #[cfg(test)]
    fn is_read_expired_for_test(&mut self, cx: &mut Context<'_>) -> bool {
        self.is_read_expired(cx)
    }

    #[cfg(test)]
    fn is_write_expired_for_test(&mut self, cx: &mut Context<'_>) -> bool {
        self.is_write_expired(cx)
    }
}

impl AsyncRead for MaxConnectionAgeIo {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();
        if this.state.is_read_expired(cx) {
            Poll::Ready(Ok(()))
        } else {
            Pin::new(&mut this.inner).poll_read(cx, buf)
        }
    }
}

impl AsyncWrite for MaxConnectionAgeIo {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let this = self.get_mut();
        if this.state.is_write_expired(cx) {
            Poll::Ready(Err(std::io::ErrorKind::BrokenPipe.into()))
        } else {
            Pin::new(&mut this.inner).poll_write(cx, buf)
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();
        if this.state.is_write_expired(cx) {
            Poll::Ready(Err(std::io::ErrorKind::BrokenPipe.into()))
        } else {
            Pin::new(&mut this.inner).poll_flush(cx)
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
    }
}

impl Connected for MaxConnectionAgeIo {
    type ConnectInfo = MaxConnectionAgeConnectInfo;

    fn connect_info(&self) -> Self::ConnectInfo {
        MaxConnectionAgeConnectInfo {
            active_requests: self.state.active_requests(),
        }
    }
}

#[derive(Clone, Debug)]
struct MaxConnectionAgeConnectInfo {
    active_requests: Arc<AtomicUsize>,
}

#[derive(Clone)]
struct MaxConnectionAgeLayer;

impl MaxConnectionAgeLayer {
    const fn new() -> Self {
        Self
    }
}

impl<S> Layer<S> for MaxConnectionAgeLayer {
    type Service = MaxConnectionAgeService<S>;

    fn layer(&self, service: S) -> Self::Service {
        MaxConnectionAgeService { service }
    }
}

#[derive(Clone)]
struct MaxConnectionAgeService<S> {
    service: S,
}

impl<S> NamedService for MaxConnectionAgeService<S>
where
    S: NamedService,
{
    const NAME: &'static str = S::NAME;
}

struct ActiveRequestGuard {
    active_requests: Arc<AtomicUsize>,
}

impl ActiveRequestGuard {
    fn new(active_requests: Arc<AtomicUsize>) -> Self {
        active_requests.fetch_add(1, Ordering::AcqRel);
        Self { active_requests }
    }
}

impl Drop for ActiveRequestGuard {
    fn drop(&mut self) {
        self.active_requests.fetch_sub(1, Ordering::AcqRel);
    }
}

#[pin_project]
struct MaxConnectionAgeBody<B> {
    #[pin]
    inner: B,
    _guard: Option<ActiveRequestGuard>,
}

impl<B> MaxConnectionAgeBody<B> {
    const fn new(inner: B, guard: Option<ActiveRequestGuard>) -> Self {
        Self {
            inner,
            _guard: guard,
        }
    }
}

impl<B> HttpBody for MaxConnectionAgeBody<B>
where
    B: HttpBody,
{
    type Data = B::Data;
    type Error = B::Error;

    fn poll_data(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Self::Data, Self::Error>>> {
        self.project().inner.poll_data(cx)
    }

    fn poll_trailers(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<Option<HeaderMap>, Self::Error>> {
        self.project().inner.poll_trailers(cx)
    }

    fn is_end_stream(&self) -> bool {
        self.inner.is_end_stream()
    }

    fn size_hint(&self) -> hyper::body::SizeHint {
        self.inner.size_hint()
    }
}

impl<S, B> Service<Request<Body>> for MaxConnectionAgeService<S>
where
    S: Service<Request<Body>, Response = Response<B>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    B: HttpBody + Send + 'static,
{
    type Response = Response<MaxConnectionAgeBody<B>>;
    type Error = S::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        let guard = req
            .extensions()
            .get::<MaxConnectionAgeConnectInfo>()
            .map(|connect_info| ActiveRequestGuard::new(Arc::clone(&connect_info.active_requests)));
        let future = self.service.call(req);

        async move {
            future
                .await
                .map(|response| response.map(|body| MaxConnectionAgeBody::new(body, guard)))
        }
        .boxed()
    }
}

pub async fn run_grpc_server<S>(
    address: SocketAddr,
    tls_settings: MaybeTlsSettings,
    tls_reloader: Option<TlsAcceptorReloader>,
    service: S,
    keepalive: GrpcKeepaliveConfig,
    shutdown: ShutdownSignal,
) -> crate::Result<()>
where
    S: Service<Request<Body>, Response = Response<BoxBody>, Error = Infallible>
        + NamedService
        + Clone
        + Send
        + 'static,
    S::Future: Send + 'static,
{
    let span = Span::current();
    let (tx, rx) = tokio::sync::oneshot::channel::<ShutdownSignalToken>();
    let listener = tls_settings.bind_reloadable(&address, tls_reloader).await?;
    let max_connection_lifetime = keepalive.max_connection_lifetime();
    let stream = listener
        .accept_stream()
        .map(move |stream| stream.map(|io| MaxConnectionAgeIo::new(io, max_connection_lifetime)));

    info!(%address, "Building gRPC server.");

    Server::builder()
        .layer(MaxConnectionAgeLayer::new())
        .layer(build_grpc_trace_layer(span.clone()))
        // This layer explicitly decompresses payloads, if compressed, and reports the number of message bytes we've
        // received if the message is processed successfully, aka `BytesReceived`. We do this because otherwise the only
        // access we have is either the event-specific bytes (the in-memory representation) or the raw bytes over the
        // wire prior to decompression... and if that case, any bytes at all, not just the ones we successfully process.
        //
        // The weaving of `tonic`, `axum`, `tower`, and `hyper` is fairly complex and there currently exists no way to
        // use independent `tower` layers when the request body itself (the body type, not the actual bytes) must be
        // modified or wrapped.. so instead of a cleaner design, we're opting here to bake it all together until the
        // crates are sufficiently flexible for us to craft a better design.
        .layer(DecompressionAndMetricsLayer)
        .add_service(service)
        .serve_with_incoming_shutdown(stream, shutdown.map(|token| tx.send(token).unwrap()))
        .await?;

    drop(rx.await);

    Ok(())
}

// This is a bit of a ugly hack to allow us to run two services on the same port.
// I just don't know how to convert the generic type with associated types into a Vec<Box<trait object>>.
pub async fn run_grpc_server_with_routes(
    address: SocketAddr,
    tls_settings: MaybeTlsSettings,
    tls_reloader: Option<TlsAcceptorReloader>,
    routes: Routes,
    keepalive: GrpcKeepaliveConfig,
    shutdown: ShutdownSignal,
) -> crate::Result<()> {
    let span = Span::current();
    let (tx, rx) = tokio::sync::oneshot::channel::<ShutdownSignalToken>();
    let listener = tls_settings.bind_reloadable(&address, tls_reloader).await?;
    let max_connection_lifetime = keepalive.max_connection_lifetime();
    let stream = listener
        .accept_stream()
        .map(move |stream| stream.map(|io| MaxConnectionAgeIo::new(io, max_connection_lifetime)));

    info!(%address, "Building gRPC server.");

    Server::builder()
        .layer(MaxConnectionAgeLayer::new())
        .layer(build_grpc_trace_layer(span.clone()))
        .layer(DecompressionAndMetricsLayer)
        .add_routes(routes)
        .serve_with_incoming_shutdown(stream, shutdown.map(|token| tx.send(token).unwrap()))
        .await?;

    drop(rx.await);

    Ok(())
}

/// Builds a [TraceLayer] configured for a gRPC server.
///
/// This layer emits gPRC specific telemetry for messages received/sent and handler duration.
pub fn build_grpc_trace_layer(
    span: Span,
) -> TraceLayer<
    SharedClassifier<GrpcErrorsAsFailures>,
    impl Fn(&Request<Body>) -> Span + Clone,
    impl Fn(&Request<Body>, &Span) + Clone,
    impl Fn(&Response<BoxBody>, Duration, &Span) + Clone,
    (),
    (),
    (),
> {
    TraceLayer::new_for_grpc()
        .make_span_with(move |request: &Request<Body>| {
            // The path is defined as “/” {service name} “/” {method name}.
            let mut path = request.uri().path().split('/');
            let service = path.nth(1).unwrap_or("_unknown");
            let method = path.next().unwrap_or("_unknown");

            // This is an error span so that the labels are always present for metrics.
            error_span!(
               parent: &span,
               "grpc-request",
               grpc_service = service,
               grpc_method = method,
            )
        })
        .on_request(Box::new(|_request: &Request<Body>, _span: &Span| {
            emit!(GrpcServerRequestReceived);
        }))
        .on_response(
            |response: &Response<BoxBody>, latency: Duration, _span: &Span| {
                emit!(GrpcServerResponseSent { response, latency });
            },
        )
        .on_failure(())
        .on_body_chunk(())
        .on_eos(())
}

#[cfg(test)]
mod tests {
    use std::future::{Ready, ready};

    use super::*;

    #[derive(Clone)]
    struct EmptyBodyService;

    impl Service<Request<Body>> for EmptyBodyService {
        type Response = Response<Body>;
        type Error = Infallible;
        type Future = Ready<Result<Self::Response, Self::Error>>;

        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, _req: Request<Body>) -> Self::Future {
            ready(Ok(Response::new(Body::empty())))
        }
    }

    #[tokio::test]
    async fn max_connection_age_service_tracks_response_body_until_drop() {
        let active_requests = Arc::new(AtomicUsize::new(0));
        let mut service = MaxConnectionAgeService {
            service: EmptyBodyService,
        };
        let mut request = Request::new(Body::empty());
        request
            .extensions_mut()
            .insert(MaxConnectionAgeConnectInfo {
                active_requests: Arc::clone(&active_requests),
            });

        assert_eq!(active_requests.load(Ordering::Acquire), 0);

        let response = service
            .call(request)
            .await
            .expect("service call should succeed");

        assert_eq!(active_requests.load(Ordering::Acquire), 1);

        drop(response);

        assert_eq!(active_requests.load(Ordering::Acquire), 0);
    }

    #[tokio::test]
    async fn max_connection_age_service_tracks_active_requests_per_connection() {
        let first_connection_active_requests = Arc::new(AtomicUsize::new(0));
        let second_connection_active_requests = Arc::new(AtomicUsize::new(0));
        let mut service = MaxConnectionAgeService {
            service: EmptyBodyService,
        };
        let mut first_request = Request::new(Body::empty());
        first_request
            .extensions_mut()
            .insert(MaxConnectionAgeConnectInfo {
                active_requests: Arc::clone(&first_connection_active_requests),
            });
        let mut second_request = Request::new(Body::empty());
        second_request
            .extensions_mut()
            .insert(MaxConnectionAgeConnectInfo {
                active_requests: Arc::clone(&second_connection_active_requests),
            });

        let first_response = service
            .call(first_request)
            .await
            .expect("first service call should succeed");

        assert_eq!(first_connection_active_requests.load(Ordering::Acquire), 1);
        assert_eq!(second_connection_active_requests.load(Ordering::Acquire), 0);

        let second_response = service
            .call(second_request)
            .await
            .expect("second service call should succeed");

        assert_eq!(first_connection_active_requests.load(Ordering::Acquire), 1);
        assert_eq!(second_connection_active_requests.load(Ordering::Acquire), 1);

        drop(second_response);

        assert_eq!(first_connection_active_requests.load(Ordering::Acquire), 1);
        assert_eq!(second_connection_active_requests.load(Ordering::Acquire), 0);

        drop(first_response);

        assert_eq!(first_connection_active_requests.load(Ordering::Acquire), 0);
        assert_eq!(second_connection_active_requests.load(Ordering::Acquire), 0);
    }

    #[tokio::test]
    async fn max_connection_age_state_stops_reads_at_deadline_before_writes() {
        let mut state = MaxConnectionAgeState::new(Some(Duration::from_millis(1)));
        let active_requests = state.active_requests();
        active_requests.fetch_add(1, Ordering::AcqRel);

        sleep(Duration::from_millis(10)).await;

        let waker = futures::task::noop_waker_ref();
        let mut cx = Context::from_waker(waker);

        assert!(state.is_read_expired_for_test(&mut cx));
        assert!(!state.is_write_expired_for_test(&mut cx));

        active_requests.fetch_sub(1, Ordering::AcqRel);

        assert!(state.is_write_expired_for_test(&mut cx));
    }
}
