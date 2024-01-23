use bytes::Bytes;

use vector_lib::event::Event;
use vector_lib::{
    byte_size_of::ByteSizeOf,
    finalization::{EventFinalizers, Finalizable},
    request_metadata::{MetaDescriptive, RequestMetadata},
};

use crate::sinks::util::{
    metadata::RequestMetadataBuilder, request_builder::EncodeResult, Compression, RequestBuilder,
};

use super::encoder::AppsignalEncoder;

#[derive(Clone)]
pub(super) struct AppsignalRequest {
    pub(super) payload: Bytes,
    pub(super) finalizers: EventFinalizers,
    pub(super) metadata: RequestMetadata,
}

impl MetaDescriptive for AppsignalRequest {
    fn get_metadata(&self) -> &RequestMetadata {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut RequestMetadata {
        &mut self.metadata
    }
}

impl Finalizable for AppsignalRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        self.finalizers.take_finalizers()
    }
}

impl ByteSizeOf for AppsignalRequest {
    fn allocated_bytes(&self) -> usize {
        self.payload.allocated_bytes() + self.finalizers.allocated_bytes()
    }
}

pub(super) struct AppsignalRequestBuilder {
    pub(super) encoder: AppsignalEncoder,
    pub(super) compression: Compression,
}

impl RequestBuilder<Vec<Event>> for AppsignalRequestBuilder {
    type Metadata = EventFinalizers;
    type Events = Vec<Event>;
    type Encoder = AppsignalEncoder;
    type Payload = Bytes;
    type Request = AppsignalRequest;
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
        AppsignalRequest {
            finalizers: metadata,
            payload: payload.into_payload(),
            metadata: request_metadata,
        }
    }
}
