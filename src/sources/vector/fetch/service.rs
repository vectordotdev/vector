use std::task::{Context, Poll};

use super::compression::VectorCompression;
use futures::{TryFutureExt, future::BoxFuture};
use http::Uri;
use hyper::client::HttpConnector;
use hyper_openssl::HttpsConnector;
use hyper_proxy::ProxyConnector;
use tonic::{body::BoxBody};
use crate::{
    Error,
    proto::vector as proto_vector,
};

#[derive(Clone, Debug)]
pub struct VectorService {
    pub client: proto_vector::Client<HyperSvc>,
}

impl VectorService {
    pub fn new(
        hyper_client: hyper::Client<ProxyConnector<HttpsConnector<HttpConnector>>, BoxBody>,
        uri: Uri,
        compression: VectorCompression,
    ) -> Self {
        let mut proto_client = proto_vector::Client::new(HyperSvc {
            uri,
            client: hyper_client,
        });

        if let Some(encoding) = compression.as_tonic_encoding() {
            proto_client = proto_client.send_compressed(encoding)
                .accept_compressed(encoding);
        }

        Self {
            client: proto_client,
        }
    }
}

#[derive(Debug)]
pub struct PullEventsRequestError {
    pub status: tonic::Status
}

impl std::fmt::Display for PullEventsRequestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Failed pull_events grpc request: ")?;
        f.write_str(&format!("{}", self.status))?;
        Ok(())
    }
}

impl std::error::Error for PullEventsRequestError {}

impl tower::Service<proto_vector::PullEventsRequest> for VectorService {
    type Response = tonic::Streaming<proto_vector::PullEventsResponse>;
    type Error = Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    // Emission of an internal event in case of errors is handled upstream by the caller.
    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        // Readiness check of the client is done through the `push_events()`
        // call happening inside `call()`. That check blocks until the client is
        // ready to perform another request.
        //
        // See: <https://docs.rs/tonic/0.4.2/tonic/client/struct.Grpc.html#method.ready>
        Poll::Ready(Ok(()))
    }

    // Emission of internal events for errors and dropped events is handled upstream by the caller.
    fn call(&mut self, list: proto_vector::PullEventsRequest) -> Self::Future {
        let mut service = self.clone();

        let future = async move {
            service
                .client
                .pull_events(list)
                .map_ok(|response| {
                    response.into_inner()
                })
                .map_err(|status| -> Self::Error {
                    Box::new(PullEventsRequestError{ status })
                })
                .await
        };

        Box::pin(future)
    }
}

#[derive(Clone, Debug)]
pub struct HyperSvc {
    uri: Uri,
    client: hyper::Client<ProxyConnector<HttpsConnector<HttpConnector>>, BoxBody>,
}

impl tower::Service<hyper::Request<BoxBody>> for HyperSvc {
    type Response = hyper::Response<hyper::Body>;
    type Error = hyper::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    // Emission of an internal event in case of errors is handled upstream by the caller.
    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    // Emission of internal events for errors and dropped events is handled upstream by the caller.
    fn call(&mut self, mut req: hyper::Request<BoxBody>) -> Self::Future {
        let uri = Uri::builder()
            .scheme(self.uri.scheme().unwrap().clone())
            .authority(self.uri.authority().unwrap().clone())
            .path_and_query(req.uri().path_and_query().unwrap().clone())
            .build()
            .unwrap();

        *req.uri_mut() = uri;

        Box::pin(self.client.request(req))
    }
}
