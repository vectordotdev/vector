//! HTTP clients that share Vector's OpenSSL TLS settings and proxy routing.
//!
//! These clients emit no internal events. The `vector` crate wraps [`HttpClient`] to add its
//! default headers and HTTP client telemetry.

#![deny(warnings)]
#![deny(clippy::all)]
#![deny(unreachable_pub)]
#![deny(unused_allocation)]
#![deny(unused_extern_crates)]
#![deny(unused_assignments)]
#![deny(unused_comparisons)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::must_use_candidate)]

mod connector;

use std::{
    convert::Infallible,
    error::Error,
    fmt,
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use bytes::Bytes;
use http::{Request, Response, header::PROXY_AUTHORIZATION};
use http_body_util::{BodyExt, Empty, Full, combinators::UnsyncBoxBody};
use hyper::body::Incoming;
use hyper_util::{
    client::legacy::{Builder, Client},
    rt::{TokioExecutor, TokioTimer},
};
use tower::Service;
use vector_core::{config::proxy::ProxyConfig, tls::MaybeTlsSettings};

use crate::connector::{ForwardProxyAlpn, HttpProxyConnector, Route, RoutePlanner};

type BoxError = Box<dyn Error + Send + Sync>;

/// The request body type of [`HttpClient`].
pub type RequestBody = UnsyncBoxBody<Bytes, BoxError>;

/// A failed HTTP request, which records whether a retry may succeed.
#[derive(Debug)]
pub struct HttpError {
    source: BoxError,
    retriable: bool,
}

impl HttpError {
    /// Wraps a retriable transport error.
    pub fn new(source: impl Error + Send + Sync + 'static) -> Self {
        Self {
            source: Box::new(source),
            retriable: true,
        }
    }

    #[must_use]
    pub const fn is_retriable(&self) -> bool {
        self.retriable
    }
}

impl fmt::Display for HttpError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "HTTP request failed: {}", self.source)
    }
}

impl Error for HttpError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.source.as_ref())
    }
}

/// Wraps a non-retriable error.
impl From<BoxError> for HttpError {
    fn from(source: BoxError) -> Self {
        Self {
            source,
            retriable: false,
        }
    }
}

#[must_use]
pub fn empty_body() -> RequestBody {
    Empty::<Bytes>::new()
        .map_err(|never: Infallible| match never {})
        .boxed_unsync()
}

pub fn full_body(bytes: Bytes) -> RequestBody {
    Full::new(bytes)
        .map_err(|never: Infallible| match never {})
        .boxed_unsync()
}

/// An HTTP/1.1 and HTTP/2 client that routes requests through the configured proxy.
///
/// Sending takes two steps, so a caller can observe the request with its proxy credentials
/// before it is sent: [`HttpClient::add_proxy_authorization`], then [`HttpClient::send`].
/// A TLS forward (non-`CONNECT`) proxy is only offered `http/1.1` ALPN.
#[derive(Clone)]
pub struct HttpClient {
    client: Client<HttpProxyConnector, RequestBody>,
    routes: Arc<RoutePlanner>,
}

impl fmt::Debug for HttpClient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("HttpClient").finish_non_exhaustive()
    }
}

impl HttpClient {
    /// Builds a client that routes requests through `proxy` and uses `tls` for HTTPS
    /// destinations and TLS proxies.
    ///
    /// # Errors
    ///
    /// Returns an error if a configured proxy URL is invalid or the TLS settings cannot
    /// build a connector.
    pub fn new(tls: MaybeTlsSettings, proxy: &ProxyConfig) -> Result<Self, BoxError> {
        let routes = Arc::new(RoutePlanner::new(proxy)?);
        let connector = HttpProxyConnector::new(tls, Arc::clone(&routes), ForwardProxyAlpn::Http1)?;
        let client = Client::builder(TokioExecutor::new()).build(connector);

        Ok(Self { client, routes })
    }

    /// Inserts the forward proxy credentials that the route of `request` needs, unless it
    /// already carries them. Call this before [`HttpClient::send`].
    ///
    /// # Errors
    ///
    /// Returns a non-retriable error if the URI cannot be routed.
    pub fn add_proxy_authorization(
        &self,
        request: &mut Request<RequestBody>,
    ) -> Result<(), HttpError> {
        add_forward_proxy_authorization(&self.routes, request).map_err(HttpError::from)
    }

    /// Sends `request` as is. It must already carry its proxy credentials; see
    /// [`HttpClient::add_proxy_authorization`].
    ///
    /// # Errors
    ///
    /// Returns a retriable error if the request fails in transport.
    pub async fn send(
        &self,
        request: Request<RequestBody>,
    ) -> Result<Response<Incoming>, HttpError> {
        self.client.request(request).await.map_err(HttpError::new)
    }
}

/// An HTTP/2-only client with the TLS and proxy routing of [`HttpClient`].
///
/// Every connection speaks HTTP/2 with prior knowledge, whatever ALPN negotiates:
///
/// - Direct `http://` destinations use h2c. Direct `https://` destinations use the TLS settings.
/// - Proxied `https://` destinations use an HTTP/1.1 `CONNECT` tunnel. The proxy connection only
///   offers `http/1.1` ALPN.
/// - Proxied `http://` destinations send HTTP/2 to the forward proxy, which must accept it.
///
/// Connections open lazily, on the first request that needs one.
pub struct Http2Client<B> {
    client: Client<HttpProxyConnector, B>,
    routes: Arc<RoutePlanner>,
}

impl<B> Clone for Http2Client<B> {
    fn clone(&self) -> Self {
        Self {
            client: self.client.clone(),
            routes: Arc::clone(&self.routes),
        }
    }
}

impl<B> fmt::Debug for Http2Client<B> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Http2Client")
            .finish_non_exhaustive()
    }
}

impl<B> Http2Client<B>
where
    B: http_body::Body + Send + Unpin + 'static,
    B::Data: Send,
    B::Error: Into<BoxError>,
{
    /// Builds the client from `builder`, which supplies the pool and HTTP/2 settings.
    ///
    /// This forces `http2_only` and installs the Tokio timers that HTTP/2 keepalive and idle
    /// connection eviction need.
    ///
    /// # Errors
    ///
    /// Returns an error if a proxy URL is invalid or the TLS settings cannot build a connector.
    pub fn new(
        tls: MaybeTlsSettings,
        proxy: &ProxyConfig,
        mut builder: Builder,
    ) -> Result<Self, BoxError> {
        let routes = Arc::new(RoutePlanner::new(proxy)?);
        let connector =
            HttpProxyConnector::new(tls, Arc::clone(&routes), ForwardProxyAlpn::Configured)?;
        let client = builder
            .http2_only(true)
            .timer(TokioTimer::new())
            .pool_timer(TokioTimer::new())
            .build(connector);

        Ok(Self { client, routes })
    }

    /// Sends `request`, adding forward proxy credentials when the route needs them.
    ///
    /// # Errors
    ///
    /// Returns a non-retriable error if the URI cannot be routed, and a retriable error if the
    /// transport fails.
    pub fn request(
        &self,
        mut request: Request<B>,
    ) -> impl Future<Output = Result<Response<Incoming>, HttpError>> + Send + 'static + use<B> {
        let response = add_forward_proxy_authorization(&self.routes, &mut request)
            .map(|()| self.client.request(request));

        async move {
            match response {
                Ok(response) => response.await.map_err(HttpError::new),
                Err(error) => Err(HttpError::from(error)),
            }
        }
    }
}

/// Sends gRPC requests, such as tonic's, over an [`Http2Client`].
pub struct GrpcTransport<B>(pub Http2Client<B>);

impl<B> Clone for GrpcTransport<B> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<B> fmt::Debug for GrpcTransport<B> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("GrpcTransport")
            .field(&self.0)
            .finish()
    }
}

impl<B> Service<Request<B>> for GrpcTransport<B>
where
    B: http_body::Body + Send + Unpin + 'static,
    B::Data: Send,
    B::Error: Into<BoxError>,
{
    type Response = Response<Incoming>;
    type Error = HttpError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    // Emission of an internal event in case of errors is handled upstream by the caller.
    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    // Emission of internal events for errors and dropped events is handled upstream by the caller.
    fn call(&mut self, request: Request<B>) -> Self::Future {
        Box::pin(self.0.request(request))
    }
}

/// Inserts the forward proxy credentials for `request` unless it already carries them.
fn add_forward_proxy_authorization<B>(
    routes: &RoutePlanner,
    request: &mut Request<B>,
) -> Result<(), BoxError> {
    if let Route::ForwardProxy {
        authorization: Some(authorization),
        ..
    } = routes.route(request.uri())?
        && !request.headers().contains_key(PROXY_AUTHORIZATION)
    {
        request
            .headers_mut()
            .insert(PROXY_AUTHORIZATION, authorization);
    }
    Ok(())
}
