use std::task::{Context, Poll};

use futures::future::BoxFuture;
use http::Uri;
use hyper::client::HttpConnector;
use hyper_openssl::HttpsConnector;
use hyper_proxy::ProxyConnector;
use tonic::body::BoxBody;
use tower::Service;

/// A Tower [`Service`] that routes gRPC requests through a Hyper HTTP/2 client,
/// substituting the scheme and authority from a fixed base URI while preserving
/// the path/query set by tonic.
///
/// Used by gRPC sinks that need to send requests to a specific endpoint using a
/// shared Hyper client.
#[derive(Clone, Debug)]
pub struct HyperGrpcService {
    pub uri: Uri,
    pub client: hyper::Client<ProxyConnector<HttpsConnector<HttpConnector>>, BoxBody>,
}

impl Service<hyper::Request<BoxBody>> for HyperGrpcService {
    type Response = hyper::Response<hyper::Body>;
    type Error = hyper::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, mut req: hyper::Request<BoxBody>) -> Self::Future {
        // SAFETY: `self.uri` is always produced by `with_default_scheme` or equivalent,
        // which guarantees a scheme and authority. Tonic always sets a path on the request URI.
        let uri = Uri::builder()
            .scheme(self.uri.scheme().expect("uri always has a scheme").clone())
            .authority(
                self.uri
                    .authority()
                    .expect("uri always has an authority")
                    .clone(),
            )
            .path_and_query(
                req.uri()
                    .path_and_query()
                    .expect("tonic request always has a path")
                    .clone(),
            )
            .build()
            .expect("uri components are always valid");

        *req.uri_mut() = uri;

        Box::pin(self.client.request(req))
    }
}
