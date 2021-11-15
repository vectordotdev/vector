use crate::Error;
use futures::future::BoxFuture;
use std::task::{Context, Poll};
use hyper_proxy::ProxyConnector;
use hyper_openssl::HttpsConnector;
use hyper::client::HttpConnector;
use tonic::body::BoxBody;
use http::Uri;

#[derive(Clone, Debug)]
pub struct VectorService {
    client: proto::Client<HyperSvc>,
    protocol: String,
    endpoint: String,
}

pub struct VectorResponse {

}

impl VectorService {
    pub fn new(
        hyper_client: hyper::Client<ProxyConnector<HttpsConnector<HttpConnector>>, BoxBody>,
        uri: Uri,
    ) -> Self {
        let (protocol, endpoint) = uri::protocol_endpoint(uri.clone());
        let proto_client = proto::Client::new(HyperSvc { uri, client });
        Self {
            client: proto_client,
            protocol,
            endpoint,
        }
    }
}

impl tower::Service<Vec<EventWrapper>> for VectorService {
    type Response = VectorResponse;
    type Error = Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        // Readiness check of the client is done through the `push_events()`
        // call happening inside `call()`. That check blocks until the client is
        // ready to perform another request.
        //
        // See: <https://docs.rs/tonic/0.4.2/tonic/client/struct.Grpc.html#method.ready>
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, events: Vec<EventWrapper>) -> Self::Future {
        let mut service = self.clone();

        let request = proto::PushEventsRequest { events };
        let byte_size = request.encoded_len();
        let future = async move {
            service.client
                .push_events(request.into_request())
                .map_ok(|_response| {
                    emit!(&EndpointBytesSent {
                        byte_size,
                        protocol: &client.protocol,
                        endpoint: &client.endpoint,
                    });
                })
                .map_err(|source| Error::Request { source })
                .await
        };

        Box::pin(future)
    }
}

//
// #[derive(Clone, Debug)]
// struct HyperSvc {
//     uri: Uri,
//     client: hyper::Client<ProxyConnector<HttpsConnector<HttpConnector>>, BoxBody>,
// }
//

//
// impl tower::Service<hyper::Request<BoxBody>> for HyperSvc {
//     type Response = hyper::Response<hyper::Body>;
//     type Error = hyper::Error;
//     type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;
//
//     fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
//         Poll::Ready(Ok(()))
//     }
//
//     fn call(&mut self, mut req: hyper::Request<BoxBody>) -> Self::Future {
//         let uri = Uri::builder()
//             .scheme(self.uri.scheme().unwrap().clone())
//             .authority(self.uri.authority().unwrap().clone())
//             .path_and_query(req.uri().path_and_query().unwrap().clone())
//             .build()
//             .unwrap();
//
//         *req.uri_mut() = uri;
//
//         Box::pin(self.client.request(req))
//     }
// }


