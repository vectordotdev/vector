use std::{
    sync::{Arc, Mutex},
    task::{Context, Poll},
    time::{Duration, Instant},
};

use futures::{TryFutureExt, future::BoxFuture};
use http::Uri;
use hyper::client::HttpConnector;
use hyper_openssl::HttpsConnector;
use hyper_proxy::ProxyConnector;
use prost::Message;
use tonic::{IntoRequest, body::BoxBody};
use tower::Service;
use vector_lib::{
    request_metadata::{GroupedCountByteSize, MetaDescriptive, RequestMetadata},
    stream::DriverResponse,
};

use super::VectorSinkError;
use crate::{
    Error,
    config::ProxyConfig,
    event::{EventFinalizers, EventStatus, Finalizable},
    http::build_proxy_connector,
    internal_events::EndpointBytesSent,
    proto::vector as proto_vector,
    sinks::util::uri,
    tls::MaybeTlsSettings,
};

struct ClientState {
    client: hyper::Client<ProxyConnector<HttpsConnector<HttpConnector>>, BoxBody>,
    created_at: Instant,
}

#[derive(Clone)]
pub struct VectorService {
    pub client: proto_vector::Client<HyperSvc>,
    pub protocol: String,
    pub endpoint: String,
    uri: Uri,
    compression: bool,
    connection_ttl: Option<Duration>,
    tls_settings: MaybeTlsSettings,
    proxy_config: ProxyConfig,
    client_state: Arc<Mutex<ClientState>>,
}

impl std::fmt::Debug for VectorService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VectorService")
            .field("protocol", &self.protocol)
            .field("endpoint", &self.endpoint)
            .field("uri", &self.uri)
            .field("compression", &self.compression)
            .field("connection_ttl", &self.connection_ttl)
            .finish_non_exhaustive()
    }
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
    pub fn new(
        hyper_client: hyper::Client<ProxyConnector<HttpsConnector<HttpConnector>>, BoxBody>,
        uri: Uri,
        compression: bool,
        connection_ttl: Option<Duration>,
        tls_settings: MaybeTlsSettings,
        proxy_config: ProxyConfig,
    ) -> Self {
        let (protocol, endpoint) = uri::protocol_endpoint(uri.clone());
        let mut proto_client = proto_vector::Client::new(HyperSvc {
            uri: uri.clone(),
            client: hyper_client.clone(),
        });

        if compression {
            proto_client = proto_client.send_compressed(tonic::codec::CompressionEncoding::Gzip);
        }

        let client_state = Arc::new(Mutex::new(ClientState {
            client: hyper_client,
            created_at: Instant::now(),
        }));

        Self {
            client: proto_client,
            protocol,
            endpoint,
            uri,
            compression,
            connection_ttl,
            tls_settings,
            proxy_config,
            client_state,
        }
    }

    fn check_and_recreate_client(&mut self) {
        if let Some(ttl) = self.connection_ttl {
            let mut state = self.client_state.lock().unwrap();
            let elapsed = state.created_at.elapsed();

            if elapsed >= ttl {
                // Recreate the client
                if let Ok(proxy) =
                    build_proxy_connector(self.tls_settings.clone(), &self.proxy_config)
                {
                    let new_client = hyper::Client::builder().http2_only(true).build(proxy);

                    state.client = new_client.clone();
                    state.created_at = Instant::now();

                    // Update the proto client with the new hyper client
                    let mut proto_client = proto_vector::Client::new(HyperSvc {
                        uri: self.uri.clone(),
                        client: new_client,
                    });

                    if self.compression {
                        proto_client =
                            proto_client.send_compressed(tonic::codec::CompressionEncoding::Gzip);
                    }

                    self.client = proto_client;
                }
            }
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
        // Check if we need to recreate the client due to TTL expiration
        self.check_and_recreate_client();

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
    client: hyper::Client<ProxyConnector<HttpsConnector<HttpConnector>>, BoxBody>,
}

impl Service<hyper::Request<BoxBody>> for HyperSvc {
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
