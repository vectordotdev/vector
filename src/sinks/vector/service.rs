use std::task::{Context, Poll};

use futures::{TryFutureExt, future::BoxFuture};
use http_1::Uri;
use hyper_1::body::Incoming;
use hyper_openssl_10::client::legacy::HttpsConnector;
use hyper_proxy2::ProxyConnector;
use hyper_util::client::legacy::{Client, connect::HttpConnector};
use prost::Message;
use tonic::{IntoRequest, body::Body as TonicBody};
use tower::Service;
use vector_lib::{
    request_metadata::{GroupedCountByteSize, MetaDescriptive, RequestMetadata},
    stream::DriverResponse,
};

use super::VectorSinkError;
use crate::{
    Error,
    event::{EventFinalizers, EventStatus, Finalizable},
    http::http_1::IntoLegacyHttp,
    internal_events::EndpointBytesSent,
    proto::vector as proto_vector,
    sinks::util::uri,
};

type HyperClient = Client<ProxyConnector<HttpsConnector<HttpConnector>>, TonicBody>;

#[derive(Clone, Debug)]
pub struct VectorService {
    pub client: proto_vector::Client<HyperSvc>,
    pub protocol: String,
    pub endpoint: String,
}

pub struct VectorResponse {
    events_byte_size: GroupedCountByteSize,
}

impl DriverResponse for VectorResponse {
    fn event_status(&self) -> EventStatus {
        EventStatus::Delivered
    }

    fn events_sent(&self) -> &GroupedCountByteSize {
        &self.events_byte_size
    }
}

#[derive(Clone, Default)]
pub struct VectorRequest {
    pub finalizers: EventFinalizers,
    pub metadata: RequestMetadata,
    pub request: proto_vector::PushEventsRequest,
}

impl Finalizable for VectorRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        self.finalizers.take_finalizers()
    }
}

impl MetaDescriptive for VectorRequest {
    fn get_metadata(&self) -> &RequestMetadata {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut RequestMetadata {
        &mut self.metadata
    }
}

impl VectorService {
    pub fn new(hyper_client: HyperClient, uri: Uri, compression: bool) -> Self {
        let (protocol, endpoint) = uri::protocol_endpoint(uri.into_legacy_http());
        let mut proto_client = proto_vector::Client::new(HyperSvc {
            uri,
            client: hyper_client,
        });

        if compression {
            proto_client = proto_client.send_compressed(tonic::codec::CompressionEncoding::Gzip);
        }
        Self {
            client: proto_client,
            protocol,
            endpoint,
        }
    }
}

impl Service<VectorRequest> for VectorService {
    type Response = VectorResponse;
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
    fn call(&mut self, mut list: VectorRequest) -> Self::Future {
        let mut service = self.clone();
        let byte_size = list.request.encoded_len();
        let metadata = std::mem::take(list.metadata_mut());
        let events_byte_size = metadata.into_events_estimated_json_encoded_byte_size();

        let future = async move {
            service
                .client
                .push_events(list.request.into_request())
                .map_ok(|_response| {
                    emit!(EndpointBytesSent {
                        byte_size,
                        protocol: &service.protocol,
                        endpoint: &service.endpoint,
                    });

                    VectorResponse { events_byte_size }
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
    client: HyperClient,
}

impl Service<http_1::Request<TonicBody>> for HyperSvc {
    type Response = http_1::Response<Incoming>;
    type Error = hyper_util::client::legacy::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    // Emission of an internal event in case of errors is handled upstream by the caller.
    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    // Emission of internal events for errors and dropped events is handled upstream by the caller.
    fn call(&mut self, mut req: http_1::Request<TonicBody>) -> Self::Future {
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
