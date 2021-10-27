use crate::http::{Auth, HttpClient};
use crate::sinks::util::UriSerde;
use futures::future::BoxFuture;
use http::StatusCode;
use snafu::Snafu;
use std::task::{Context, Poll};
use tower::Service;
use tracing_futures::Instrument;
use vector_core::buffers::Ackable;
use vector_core::event::{EventFinalizers, EventStatus, Finalizable};

#[derive(Debug, Snafu)]
pub enum LokiError {
    #[snafu(display("Server responded with an error: {}", code))]
    ServerError { code: StatusCode },
    #[snafu(display("Failed to make HTTP(S) request: {}", error))]
    HttpError { error: crate::http::HttpError },
}

#[derive(Debug, Snafu)]
pub enum LokiResponse {
    Success,
}

impl AsRef<EventStatus> for LokiResponse {
    fn as_ref(&self) -> &EventStatus {
        &EventStatus::Delivered
    }
}

pub struct LokiRequest {
    pub batch_size: usize,
    pub finalizers: EventFinalizers,
    pub payload: Vec<u8>,
    pub tenant_id: Option<String>,
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

                    match status {
                        StatusCode::NO_CONTENT => Ok(LokiResponse::Success),
                        code => Err(LokiError::ServerError { code }),
                    }
                }
                Err(error) => Err(LokiError::HttpError { error }),
            }
        })
    }
}
