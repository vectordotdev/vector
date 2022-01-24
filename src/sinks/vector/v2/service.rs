use std::task::{Context, Poll};

use futures::{future::BoxFuture, TryFutureExt};
use http::Uri;
use hyper::client::HttpConnector;
use hyper_openssl::HttpsConnector;
use hyper_proxy::ProxyConnector;
use prost::Message;
use proto_event::EventWrapper;
use tonic::{body::BoxBody, IntoRequest};
use vector_core::{
    buffers::Ackable, event::proto as proto_event, internal_event::EventsSent,
    stream::DriverResponse,
};

use crate::{
    event::{EventFinalizers, EventStatus, Finalizable},
    internal_events::EndpointBytesSent,
    proto::vector as proto_vector,
    sinks::{util::uri, vector::v2::VectorSinkError},
    Error,
};

#[derive(Clone, Debug)]
pub struct VectorService {
    pub client: proto_vector::Client<HyperSvc>,
    pub protocol: String,
    pub endpoint: String,
}

pub struct VectorResponse {
    events_count: usize,
    events_byte_size: usize,
}

impl DriverResponse for VectorResponse {
    fn event_status(&self) -> EventStatus {
        EventStatus::Delivered
    }

    fn events_sent(&self) -> EventsSent {
        EventsSent {
            count: self.events_count,
            byte_size: self.events_byte_size,
            output: None,
        }
    }
}

#[derive(Clone, Default)]
pub struct VectorRequest {
    pub events: Vec<EventWrapper>,
    pub finalizers: EventFinalizers,
    pub events_byte_size: usize,
}

impl Ackable for VectorRequest {
    fn ack_size(&self) -> usize {
        self.events.len()
    }
}

impl Finalizable for VectorRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        self.finalizers.take_finalizers()
    }
}

impl VectorService {
    pub fn new(
        hyper_client: hyper::Client<ProxyConnector<HttpsConnector<HttpConnector>>, BoxBody>,
        uri: Uri,
    ) -> Self {
        let (protocol, endpoint) = uri::protocol_endpoint(uri.clone());
        let proto_client = proto_vector::Client::new(HyperSvc {
            uri,
            client: hyper_client,
        });
        Self {
            client: proto_client,
            protocol,
            endpoint,
        }
    }
}

impl tower::Service<VectorRequest> for VectorService {
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

    fn call(&mut self, list: VectorRequest) -> Self::Future {
        let mut service = self.clone();
        let events_count = list.events.len();

        let request = proto_vector::PushEventsRequest {
            events: list.events,
        };
        let byte_size = request.encoded_len();
        let future = async move {
            service
                .client
                .push_events(request.into_request())
                .map_ok(|_response| {
                    emit!(&EndpointBytesSent {
                        byte_size,
                        protocol: &service.protocol,
                        endpoint: &service.endpoint,
                    });
                    VectorResponse {
                        events_count,
                        events_byte_size: 0,
                    }
                })
                .map_err(|source| VectorSinkError::Request { source }.into())
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

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

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
