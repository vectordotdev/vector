//! `RequestBuilder` implementation for the `opentelemetry` sink.

use bytes::Bytes;
use std::io;

use crate::sinks::{prelude::*, util::http::HttpRequest};

use super::encoder::OpentelemetryEncoder;

pub(super) struct OpentelemetryRequestBuilder {
    pub(super) encoder: OpentelemetryEncoder,
}

impl RequestBuilder<Vec<Event>> for OpentelemetryRequestBuilder {
    type Metadata = EventFinalizers;
    type Events = Vec<Event>;
    type Encoder = OpentelemetryEncoder;
    type Payload = Bytes;
    type Request = HttpRequest<()>;
    type Error = io::Error;

    fn compression(&self) -> Compression {
        Compression::None
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoder
    }

    fn split_input(
        &self,
        mut events: Vec<Event>,
    ) -> (Self::Metadata, RequestMetadataBuilder, Self::Events) {
        let finalizers = events.take_finalizers();
        let builder = RequestMetadataBuilder::from_events(&events);
        (finalizers, builder, events)
    }

    fn build_request(
        &self,
        metadata: Self::Metadata,
        request_metadata: RequestMetadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        HttpRequest::new(payload.into_payload(), metadata, request_metadata, ())
    }
}
