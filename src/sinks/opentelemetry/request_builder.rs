//! `RequestBuilder` implementation for the `opentelemetry` sink.

use bytes::Bytes;
use std::io;

use crate::sinks::{prelude::*, util::http::HttpRequest};

use super::encoder::OpentelemetryEncoder;

pub(super) struct OpentelemetryRequestBuilder {
    pub(super) encoder: OpentelemetryEncoder,
}

use super::sink::PartitionKey;

impl RequestBuilder<(PartitionKey, Vec<Event>)> for OpentelemetryRequestBuilder {
    type Metadata = (PartitionKey, EventFinalizers);
    type Events = Vec<Event>;
    type Encoder = OpentelemetryEncoder;
    type Payload = Bytes;
    type Request = HttpRequest<PartitionKey>;
    type Error = io::Error;

    fn compression(&self) -> Compression {
        Compression::None
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoder
    }

    fn split_input(
        &self,
        input: (PartitionKey, Vec<Event>),
    ) -> (Self::Metadata, RequestMetadataBuilder, Self::Events) {
        let (key, mut events) = input;
        let finalizers = events.take_finalizers();
        let builder = RequestMetadataBuilder::from_events(&events);
        ((key, finalizers), builder, events)
    }

    fn build_request(
        &self,
        metadata: Self::Metadata,
        request_metadata: RequestMetadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        let (key, finalizers) = metadata;
        HttpRequest::new(
            payload.into_payload(),
            finalizers,
            request_metadata,
            PartitionKey {
                endpoint: key.endpoint,
            },
        )
    }
}
