use super::{NewRelicCredentials, NewRelicSinkError};
use crate::{http::HttpClient, sinks::util::Compression};
use futures::future::BoxFuture;
use http::{
    header::{CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE},
    Request,
};
use hyper::Body;
use std::{
    fmt::Debug,
    sync::Arc,
    task::{Context, Poll},
};
use tower::Service;
use tracing::Instrument;
use vector_core::{
    buffers::Ackable,
    event::{EventFinalizers, EventStatus, Finalizable},
    internal_event::EventsSent,
    stream::DriverResponse,
};

#[derive(Debug, Clone)]
pub struct NewRelicApiRequest {
    pub batch_size: usize,
    pub finalizers: EventFinalizers,
    pub credentials: Arc<NewRelicCredentials>,
    pub payload: Vec<u8>,
    pub compression: Compression,
}

impl Ackable for NewRelicApiRequest {
    fn ack_size(&self) -> usize {
        self.batch_size
    }
}

impl Finalizable for NewRelicApiRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.finalizers)
    }
}

#[derive(Debug)]
pub struct NewRelicApiResponse {
    event_status: EventStatus,
    count: usize,
    events_byte_size: usize,
}

impl DriverResponse for NewRelicApiResponse {
    fn event_status(&self) -> EventStatus {
        self.event_status
    }

    fn events_sent(&self) -> EventsSent {
        EventsSent {
            count: self.count,
            byte_size: self.events_byte_size,
            output: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct NewRelicApiService {
    pub client: HttpClient,
}

impl Service<NewRelicApiRequest> for NewRelicApiService {
    type Response = NewRelicApiResponse;
    type Error = NewRelicSinkError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: NewRelicApiRequest) -> Self::Future {
        debug!("Sending {} events.", request.batch_size);

        let mut client = self.client.clone();

        let uri = request.credentials.get_uri();

        let http_request = Request::post(&uri)
            .header(CONTENT_TYPE, "application/json")
            .header("Api-Key", request.credentials.license_key.clone());

        let http_request = if let Some(ce) = request.compression.content_encoding() {
            http_request.header(CONTENT_ENCODING, ce)
        } else {
            http_request
        };

        let payload_len = request.payload.len();
        let http_request = http_request
            .header(CONTENT_LENGTH, payload_len)
            .body(Body::from(request.payload))
            .expect("building HTTP request failed unexpectedly");

        Box::pin(async move {
            match client.call(http_request).in_current_span().await {
                Ok(_) => Ok(NewRelicApiResponse {
                    event_status: EventStatus::Delivered,
                    count: request.batch_size,
                    events_byte_size: payload_len,
                }),
                Err(_) => Err(NewRelicSinkError::new("HTTP request error")),
            }
        })
    }
}
