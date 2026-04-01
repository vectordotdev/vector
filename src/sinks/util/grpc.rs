use std::task::{Context, Poll};

use futures::future::BoxFuture;
use http::{
    Uri,
    uri::{Authority, PathAndQuery, Scheme},
};
use hyper::client::HttpConnector;
use hyper_openssl::HttpsConnector;
use hyper_proxy::ProxyConnector;
use tonic::body::BoxBody;
use tower::Service;

/// Adds a default scheme to a URI that lacks one, and validates that the URI has an authority.
///
/// Returns the URI unchanged if a scheme is already present. Otherwise prepends
/// `https` when `tls` is `true`, or `http` when `false`. Also sets the
/// path-and-query to `/` if missing.
///
/// Returns an error if the URI has no authority (host), since `HyperGrpcService`
/// requires one and would otherwise panic at request time.
pub(crate) fn with_default_scheme(uri: Uri, tls: bool) -> crate::Result<Uri> {
    let uri = if uri.scheme().is_none() {
        let mut parts = uri.into_parts();
        parts.scheme = Some(if tls { Scheme::HTTPS } else { Scheme::HTTP });
        if parts.path_and_query.is_none() {
            parts.path_and_query = Some(PathAndQuery::from_static("/"));
        }
        Uri::from_parts(parts)?
    } else {
        uri
    };
    if uri.authority().is_none() {
        return Err(format!(
            "gRPC URI {uri:?} has no host; expected \"scheme://host:port\""
        )
        .into());
    }
    Ok(uri)
}

/// A Tower [`Service`] that routes gRPC requests through a Hyper HTTP/2 client,
/// substituting the scheme and authority from a fixed base URI while preserving
/// the path set by tonic.
///
/// If the configured URI includes a path prefix (e.g. `https://gateway/grpc`),
/// that prefix is prepended to every tonic RPC path so that requests reach the
/// correct backend through a reverse proxy.
///
/// Used by gRPC sinks that need to send requests to a specific endpoint using a
/// shared Hyper client.
#[derive(Clone, Debug)]
pub struct HyperGrpcService {
    scheme: Scheme,
    authority: Authority,
    /// Path prefix extracted from the configured URI (e.g. `"/grpc"`), or empty
    /// string when the URI has no meaningful path. Never ends with `/` so it can
    /// be concatenated directly with tonic's `/ServiceName/Method` paths.
    path_prefix: String,
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
        // Strip trailing slash so we can concatenate directly with tonic's leading-slash paths.
        let path_prefix = uri.path().trim_end_matches('/').to_owned();
        Self {
            scheme,
            authority,
            path_prefix,
            client,
        }
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
        // Tonic sets the RPC method path (e.g. `/pkg.Service/Method`). Prepend any
        // configured path prefix so requests reach the right backend through a proxy.
        // Falls back to "/" if tonic omits the path (it never does in practice).
        let rpc_path = req
            .uri()
            .path_and_query()
            .map(|pq| pq.as_str())
            .unwrap_or("/");
        let full_path: PathAndQuery = if self.path_prefix.is_empty() {
            rpc_path
                .parse()
                .unwrap_or_else(|_| PathAndQuery::from_static("/"))
        } else {
            format!("{}{}", self.path_prefix, rpc_path)
                .parse()
                .unwrap_or_else(|_| PathAndQuery::from_static("/"))
        };
        let uri = Uri::builder()
            .scheme(self.scheme.clone())
            .authority(self.authority.clone())
            .path_and_query(full_path)
            .build()
            .expect("pre-validated scheme and authority always produce a valid URI");

        *req.uri_mut() = uri;

        Box::pin(self.client.request(req))
    }
}
