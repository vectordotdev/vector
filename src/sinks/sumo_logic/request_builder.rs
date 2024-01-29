//! `RequestBuilder` implementation for the `sumo_logic` sink.

use bytes::Bytes;
use vector_lib::codecs::encoding::Framer;

use crate::sinks::{prelude::*, util::http::HttpRequest};

pub(super) struct SumoLogicRequestBuilder {
    pub(super) encoder: (Transformer, Encoder<Framer>),
}

impl RequestBuilder<Vec<Event>> for SumoLogicRequestBuilder {
    type Metadata = EventFinalizers;
    type Events = Vec<Event>;
    type Encoder = (Transformer, Encoder<Framer>);
    type Payload = Bytes;
    type Request = HttpRequest;
    type Error = std::io::Error;

    fn compression(&self) -> Compression {
        Compression::None
    }
    fn encoder(&self) -> &Self::Encoder {
        &self.encoder
    }
    fn split_input(
        &self,
        mut input: Vec<Event>,
    ) -> (Self::Metadata, RequestMetadataBuilder, Self::Events) {
        let finalizers = input.take_finalizers();
        let metadata_builder = RequestMetadataBuilder::from_events(&input);
        (finalizers, metadata_builder, input)
    }

    fn build_request(
        &self,
        metadata: Self::Metadata,
        request_metadata: RequestMetadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        HttpRequest::new(payload.into_payload(), metadata, request_metadata)
    }
}
