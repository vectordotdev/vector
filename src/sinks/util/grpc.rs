use std::task::{Context, Poll};

use futures::future::BoxFuture;
use http::{Uri, uri::{Authority, PathAndQuery, Scheme}};
use hyper::client::HttpConnector;
use hyper_openssl::HttpsConnector;
use hyper_proxy::ProxyConnector;
use tonic::body::BoxBody;
use tower::Service;

/// Adds a default scheme to a URI that lacks one.
///
/// Returns the URI unchanged if a scheme is already present. Otherwise prepends
/// `https` when `tls` is `true`, or `http` when `false`. Also sets the
/// path-and-query to `/` if missing.
pub(crate) fn with_default_scheme(uri: Uri, tls: bool) -> crate::Result<Uri> {
    if uri.scheme().is_none() {
        let mut parts = uri.into_parts();
        parts.scheme = Some(if tls { Scheme::HTTPS } else { Scheme::HTTP });
        if parts.path_and_query.is_none() {
            parts.path_and_query = Some(PathAndQuery::from_static("/"));
        }
        Ok(Uri::from_parts(parts)?)
    } else {
        Ok(uri)
    }
}

/// A Tower [`Service`] that routes gRPC requests through a Hyper HTTP/2 client,
/// substituting the scheme and authority from a fixed base URI while preserving
/// the path/query set by tonic.
///
/// Used by gRPC sinks that need to send requests to a specific endpoint using a
/// shared Hyper client.
#[derive(Clone, Debug)]
pub struct HyperGrpcService {
    scheme: Scheme,
    authority: Authority,
    pub client: hyper::Client<ProxyConnector<HttpsConnector<HttpConnector>>, BoxBody>,
}

impl HyperGrpcService {
    /// Creates a new [`HyperGrpcService`].
    ///
    /// # Panics
    ///
    /// Panics at construction time if `uri` lacks a scheme or authority. Always supply a
    /// URI produced by `with_default_scheme`, which guarantees both components are present.
    /// Panicking here (once, at startup) is preferable to panicking inside the hot-path
    /// `Service::call` implementation on every request.
    pub fn new(
        uri: Uri,
        client: hyper::Client<ProxyConnector<HttpsConnector<HttpConnector>>, BoxBody>,
    ) -> Self {
        let scheme = uri
            .scheme()
            .expect("gRPC service URI must have a scheme — supply a URI from `with_default_scheme`")
            .clone();
        let authority = uri
            .authority()
            .expect("gRPC service URI must have an authority (host:port) — supply a URI from `with_default_scheme`")
            .clone();
        Self { scheme, authority, client }
    }
}

impl Service<hyper::Request<BoxBody>> for HyperGrpcService {
    type Response = hyper::Response<hyper::Body>;
    type Error = hyper::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, mut req: hyper::Request<BoxBody>) -> Self::Future {
        // scheme and authority are pre-validated at construction; path_and_query falls back
        // to "/" if tonic omits it (it never does, but we avoid a panic either way).
        let uri = Uri::builder()
            .scheme(self.scheme.clone())
            .authority(self.authority.clone())
            .path_and_query(
                req.uri()
                    .path_and_query()
                    .cloned()
                    .unwrap_or_else(|| PathAndQuery::from_static("/")),
            )
            .build()
            .expect("pre-validated scheme and authority always produce a valid URI");

        *req.uri_mut() = uri;

        Box::pin(self.client.request(req))
    }
}
