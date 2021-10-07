use crate::http::HttpClient;
use futures::future::BoxFuture;
use http::{Request, StatusCode};
use hyper::Body;
use snafu::Snafu;
use std::task::{Context, Poll};
use tower::Service;
use tracing_futures::Instrument;
use vector_core::event::EventStatus;

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("Server responded with an error: {}", code))]
    ServerError { code: StatusCode },
    #[snafu(display("Failed to make HTTP(S) request: {}", error))]
    HttpError { error: crate::http::HttpError },
}

#[derive(Debug, Snafu)]
pub enum Response {
    Success,
}

impl AsRef<EventStatus> for Response {
    fn as_ref(&self) -> &EventStatus {
        &EventStatus::Delivered
    }
}

#[derive(Debug, Clone)]
pub struct LokiService {
    client: HttpClient,
}

impl LokiService {
    pub const fn new(client: HttpClient) -> Self {
        Self { client }
    }
}

impl Service<Request<Body>> for LokiService {
    type Response = Response;
    type Error = Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: Request<Body>) -> Self::Future {
        let mut client = self.client.clone();

        Box::pin(async move {
            match client.call(request).in_current_span().await {
                Ok(response) => {
                    let status = response.status();

                    match status {
                        StatusCode::NO_CONTENT => Ok(Response::Success),
                        code => Err(Error::ServerError { code }),
                    }
                }
                Err(error) => Err(Error::HttpError { error }),
            }
        })
    }
}
