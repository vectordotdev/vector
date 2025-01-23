//! `RequestBuilder` implementation for the `keep` sink.

use bytes::Bytes;
use std::io;

use crate::sinks::{prelude::*, util::http::HttpRequest};

use super::encoder::KeepEncoder;

pub(super) struct KeepRequestBuilder {
    pub(super) encoder: KeepEncoder,
    pub(super) compression: Compression,
}

impl RequestBuilder<Vec<Event>> for KeepRequestBuilder {
    type Metadata = EventFinalizers;
    type Events = Vec<Event>;
    type Encoder = KeepEncoder;
    type Payload = Bytes;
    type Request = HttpRequest<()>;
    type Error = io::Error;

    fn compression(&self) -> Compression {
        self.compression
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
