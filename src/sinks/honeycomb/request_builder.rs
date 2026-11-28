//! `RequestBuilder` implementation for the `honeycomb` sink.

use std::io;

use bytes::Bytes;

use super::encoder::HoneycombEncoder;
use crate::sinks::{prelude::*, util::http::HttpRequest};

pub(super) struct HoneycombRequestBuilder {
    pub(super) encoder: HoneycombEncoder,
    pub(super) compression: Compression,
}

impl RequestBuilder<Vec<Event>> for HoneycombRequestBuilder {
    type Metadata = ();
    type Events = Vec<Event>;
    type Encoder = HoneycombEncoder;
    type Payload = Bytes;
    type Request = HttpRequest<()>;
    type Error = io::Error;

    fn compression(&self) -> Compression {
        self.compression
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoder
    }

    fn split_input(&self, mut events: Vec<Event>) -> RequestParts<Self::Metadata, Self::Events> {
        let finalizers = events.take_finalizers();
        let builder = RequestMetadataBuilder::from_events(&events);
        RequestParts {
            metadata: (),
            request_metadata_builder: builder,
            events,
            finalizers,
        }
    }

    fn build_request(
        &self,
        (): Self::Metadata,
        finalizers: EventFinalizers,
        request_metadata: RequestMetadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        HttpRequest::new(payload.into_payload(), finalizers, request_metadata, ())
    }
}
