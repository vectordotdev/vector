//! `RequestBuilder` implementation for the `sentry` sink.

use bytes::Bytes;
use std::io;

use crate::{
    event::Event,
    sinks::{prelude::*, util::http::HttpRequest},
};

use super::encoder::SentryEncoder;

#[derive(Clone)]
pub(super) struct SentryRequestBuilder {
    pub(super) encoder: SentryEncoder,
}

impl SentryRequestBuilder {
    pub(super) const fn new(encoder: SentryEncoder) -> Self {
        Self { encoder }
    }
}

impl RequestBuilder<Vec<Event>> for SentryRequestBuilder {
    type Metadata = EventFinalizers;
    type Events = Vec<Event>;
    type Encoder = SentryEncoder;
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
