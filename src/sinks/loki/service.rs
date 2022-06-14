use std::task::{Context, Poll};

use bytes::Bytes;
use futures::future::BoxFuture;
use http::StatusCode;
use snafu::Snafu;
use tower::Service;
use tracing::Instrument;
use vector_common::internal_event::BytesSent;
use vector_core::{
    buffers::Ackable,
    event::{EventFinalizers, EventStatus, Finalizable},
    internal_event::EventsSent,
    stream::DriverResponse,
};

use crate::{
    http::{get_http_scheme_from_uri, Auth, HttpClient},
    sinks::util::{metadata::RequestMetadata, retries::RetryLogic, Compression, UriSerde},
};

#[derive(Clone)]
pub struct LokiRetryLogic;

impl RetryLogic for LokiRetryLogic {
    type Error = LokiError;
    type Response = LokiResponse;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        match error {
            LokiError::ServerError { code } => match *code {
                StatusCode::TOO_MANY_REQUESTS => true,
                StatusCode::NOT_IMPLEMENTED => false,
                _ if code.is_server_error() => true,
                _ => false,
            },
            LokiError::HttpError { .. } => true,
        }
    }
}

#[derive(Debug, Snafu)]
pub enum LokiError {
    #[snafu(display("Server responded with an error: {}", code))]
    ServerError { code: StatusCode },
    #[snafu(display("Failed to make HTTP(S) request: {}", error))]
    HttpError { error: crate::http::HttpError },
}

#[derive(Debug, Snafu)]
pub struct LokiResponse {
    protocol: &'static str,
    metadata: RequestMetadata,
}

impl DriverResponse for LokiResponse {
    fn event_status(&self) -> EventStatus {
        EventStatus::Delivered
    }

    fn events_sent(&self) -> EventsSent {
        EventsSent {
            count: self.metadata.event_count(),
            byte_size: self.metadata.events_byte_size(),
            output: None,
        }
    }

    fn bytes_sent(&self) -> Option<BytesSent> {
        Some(BytesSent {
            byte_size: self.metadata.request_encoded_size(),
            protocol: self.protocol,
        })
    }
}

#[derive(Clone)]
pub struct LokiRequest {
    pub compression: Compression,
    pub finalizers: EventFinalizers,
    pub payload: Bytes,
    pub tenant_id: Option<String>,
    pub metadata: RequestMetadata,
}

impl Ackable for LokiRequest {
    fn ack_size(&self) -> usize {
        self.metadata.event_count()
    }
}

impl Finalizable for LokiRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        self.finalizers.take_finalizers()
    }
}

#[derive(Debug, Clone)]
pub struct LokiService {
    endpoint: UriSerde,
    client: HttpClient,
}

impl LokiService {
    pub fn new(client: HttpClient, endpoint: UriSerde, auth: Option<Auth>) -> crate::Result<Self> {
        let endpoint = endpoint.append_path("loki/api/v1/push")?.with_auth(auth);

        Ok(Self { client, endpoint })
    }
}

impl Service<LokiRequest> for LokiService {
    type Response = LokiResponse;
    type Error = LokiError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: LokiRequest) -> Self::Future {
        let mut req =
            http::Request::post(&self.endpoint.uri).header("Content-Type", "application/json");
        let protocol = get_http_scheme_from_uri(&self.endpoint.uri);

        if let Some(tenant_id) = request.tenant_id {
            req = req.header("X-Scope-OrgID", tenant_id);
        }

        if let Some(ce) = request.compression.content_encoding() {
            req = req.header("Content-Encoding", ce);
        }

        let body = hyper::Body::from(request.payload);
        let mut req = req.body(body).unwrap();

        if let Some(auth) = &self.endpoint.auth {
            auth.apply(&mut req);
        }

        let mut client = self.client.clone();

        let metadata = request.metadata;
        Box::pin(async move {
            match client.call(req).in_current_span().await {
                Ok(response) => {
                    let status = response.status();

                    if status.is_success() {
                        Ok(LokiResponse { protocol, metadata })
                    } else {
                        Err(LokiError::ServerError { code: status })
                    }
                }
                Err(error) => Err(LokiError::HttpError { error }),
            }
        })
    }
}
