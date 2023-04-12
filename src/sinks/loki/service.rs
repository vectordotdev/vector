use std::task::{Context, Poll};

use bytes::Bytes;
use futures::future::BoxFuture;
use http::StatusCode;
use snafu::Snafu;
use tower::Service;
use tracing::Instrument;
use vector_common::request_metadata::{MetaDescriptive, RequestMetadata};
use vector_core::{
    event::{EventFinalizers, EventStatus, Finalizable},
    internal_event::CountByteSize,
    stream::DriverResponse,
};

use crate::sinks::loki::config::{CompressionConfigAdapter, ExtendedCompression};
use crate::{
    http::{Auth, HttpClient},
    sinks::util::{retries::RetryLogic, UriSerde},
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
    metadata: RequestMetadata,
}

impl DriverResponse for LokiResponse {
    fn event_status(&self) -> EventStatus {
        EventStatus::Delivered
    }

    fn events_sent(&self) -> CountByteSize {
        CountByteSize(
            self.metadata.event_count(),
            self.metadata.events_estimated_json_encoded_byte_size(),
        )
    }

    fn bytes_sent(&self) -> Option<usize> {
        Some(self.metadata.request_encoded_size())
    }
}

#[derive(Clone)]
pub struct LokiRequest {
    pub compression: CompressionConfigAdapter,
    pub finalizers: EventFinalizers,
    pub payload: Bytes,
    pub tenant_id: Option<String>,
    pub metadata: RequestMetadata,
}

impl Finalizable for LokiRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        self.finalizers.take_finalizers()
    }
}

impl MetaDescriptive for LokiRequest {
    fn get_metadata(&self) -> RequestMetadata {
        self.metadata
    }
}

#[derive(Debug, Clone)]
pub struct LokiService {
    endpoint: UriSerde,
    client: HttpClient,
}

impl LokiService {
    pub fn new(
        client: HttpClient,
        endpoint: UriSerde,
        path: String,
        auth: Option<Auth>,
    ) -> crate::Result<Self> {
        let endpoint = endpoint.append_path(&path)?.with_auth(auth);

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
        let content_type = match request.compression {
            CompressionConfigAdapter::Original(_) => "application/json",
            CompressionConfigAdapter::Extended(ExtendedCompression::Snappy) => {
                "application/x-protobuf"
            }
        };
        let mut req = http::Request::post(&self.endpoint.uri).header("Content-Type", content_type);

        let metadata = request.get_metadata();

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

        Box::pin(async move {
            match client.call(req).in_current_span().await {
                Ok(response) => {
                    let status = response.status();

                    if status.is_success() {
                        Ok(LokiResponse { metadata })
                    } else {
                        Err(LokiError::ServerError { code: status })
                    }
                }
                Err(error) => Err(LokiError::HttpError { error }),
            }
        })
    }
}
