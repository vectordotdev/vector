use std::task::{Context, Poll};

use futures::future::BoxFuture;
use http::StatusCode;
use snafu::Snafu;
use tower::Service;
use tracing_futures::Instrument;
use vector_core::{
    buffers::Ackable,
    event::{EventFinalizers, EventStatus, Finalizable},
    internal_event::EventsSent,
    stream::DriverResponse,
};

use crate::{
    http::{Auth, HttpClient},
    sinks::util::{Compression, UriSerde},
};

#[derive(Debug, Snafu)]
pub enum LokiError {
    #[snafu(display("Server responded with an error: {}", code))]
    ServerError { code: StatusCode },
    #[snafu(display("Failed to make HTTP(S) request: {}", error))]
    HttpError { error: crate::http::HttpError },
}

#[derive(Debug, Snafu)]
pub struct LokiResponse {
    batch_size: usize,
    events_byte_size: usize,
}

impl DriverResponse for LokiResponse {
    fn event_status(&self) -> EventStatus {
        EventStatus::Delivered
    }

    fn events_sent(&self) -> EventsSent {
        EventsSent {
            count: self.batch_size,
            byte_size: self.events_byte_size,
            output: None,
        }
    }
}

pub struct LokiRequest {
    pub compression: Compression,
    pub batch_size: usize,
    pub finalizers: EventFinalizers,
    pub payload: Vec<u8>,
    pub tenant_id: Option<String>,
    pub events_byte_size: usize,
}

impl Ackable for LokiRequest {
    fn ack_size(&self) -> usize {
        self.batch_size
    }
}

impl Finalizable for LokiRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.finalizers)
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

        let batch_size = request.batch_size;
        let events_byte_size = request.events_byte_size;
        Box::pin(async move {
            match client.call(req).in_current_span().await {
                Ok(response) => {
                    let status = response.status();

                    if status.is_success() {
                        Ok(LokiResponse {
                            batch_size,
                            events_byte_size,
                        })
                    } else {
                        Err(LokiError::ServerError { code: status })
                    }
                }
                Err(error) => Err(LokiError::HttpError { error }),
            }
        })
    }
}
