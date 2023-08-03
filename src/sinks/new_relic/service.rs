use std::{
    fmt::Debug,
    sync::Arc,
    task::{Context, Poll},
};

use bytes::Bytes;
use http::{
    header::{CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE},
    Request,
};
use hyper::Body;
use tracing::Instrument;

use super::{NewRelicCredentials, NewRelicSinkError};
use crate::sinks::prelude::*;
use crate::{http::HttpClient, sinks::util::Compression};

#[derive(Debug, Clone)]
pub struct NewRelicApiRequest {
    pub metadata: RequestMetadata,
    pub finalizers: EventFinalizers,
    pub credentials: Arc<NewRelicCredentials>,
    pub payload: Bytes,
    pub compression: Compression,
}

impl Finalizable for NewRelicApiRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.finalizers)
    }
}

impl MetaDescriptive for NewRelicApiRequest {
    fn get_metadata(&self) -> &RequestMetadata {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut RequestMetadata {
        &mut self.metadata
    }
}

#[derive(Debug)]
pub struct NewRelicApiResponse {
    event_status: EventStatus,
    metadata: RequestMetadata,
}

impl DriverResponse for NewRelicApiResponse {
    fn event_status(&self) -> EventStatus {
        self.event_status
    }

    fn events_sent(&self) -> &GroupedCountByteSize {
        self.metadata.events_estimated_json_encoded_byte_size()
    }

    fn bytes_sent(&self) -> Option<usize> {
        Some(self.metadata.request_encoded_size())
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
        let metadata = request.get_metadata().clone();
        let http_request = http_request
            .header(CONTENT_LENGTH, payload_len)
            .body(Body::from(request.payload))
            .expect("building HTTP request failed unexpectedly");

        Box::pin(async move {
            match client.call(http_request).in_current_span().await {
                Ok(_) => Ok(NewRelicApiResponse {
                    event_status: EventStatus::Delivered,
                    metadata,
                }),
                Err(_) => Err(NewRelicSinkError::new("HTTP request error")),
            }
        })
    }
}
