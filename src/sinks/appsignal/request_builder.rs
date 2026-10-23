use bytes::Bytes;
use vector_lib::{
    event::Event,
    finalization::{EventFinalizers, Finalizable},
    request_metadata::RequestMetadata,
};

use super::encoder::AppsignalEncoder;
use crate::sinks::util::{
    Compression, RequestBuilder, http::HttpRequest, metadata::RequestMetadataBuilder,
    request_builder::EncodeResult,
};

pub(super) struct AppsignalRequestBuilder {
    pub(super) encoder: AppsignalEncoder,
    pub(super) compression: Compression,
}

impl RequestBuilder<Vec<Event>> for AppsignalRequestBuilder {
    type Metadata = EventFinalizers;
    type Events = Vec<Event>;
    type Encoder = AppsignalEncoder;
    type Payload = Bytes;
    type Request = HttpRequest<()>;
    type Error = std::io::Error;

    fn compression(&self) -> Compression {
        self.compression
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
        HttpRequest::new(payload.into_payload(), metadata, request_metadata, ())
    }
}
