use std::io;

use bytes::Bytes;
use vector_lib::codecs::encoding::Framer;
use vector_lib::event::Event;
use vector_lib::finalization::{EventFinalizers, Finalizable};
use vector_lib::request_metadata::RequestMetadata;

use crate::sinks::util::Compression;
use crate::{
    codecs::{Encoder, Transformer},
    sinks::util::{
        metadata::RequestMetadataBuilder, request_builder::EncodeResult, RequestBuilder,
    },
};

use super::service::DatabendRequest;

#[derive(Clone)]
pub struct DatabendRequestBuilder {
    compression: Compression,
    encoder: (Transformer, Encoder<Framer>),
}

impl DatabendRequestBuilder {
    pub const fn new(compression: Compression, encoder: (Transformer, Encoder<Framer>)) -> Self {
        Self {
            compression,
            encoder,
        }
    }
}

impl RequestBuilder<Vec<Event>> for DatabendRequestBuilder {
    type Metadata = EventFinalizers;
    type Events = Vec<Event>;
    type Encoder = (Transformer, Encoder<Framer>);
    type Payload = Bytes;
    type Request = DatabendRequest;
    type Error = io::Error;

    fn compression(&self) -> Compression {
        self.compression
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoder
    }

    fn split_input(
        &self,
        input: Vec<Event>,
    ) -> (Self::Metadata, RequestMetadataBuilder, Self::Events) {
        let mut events = input;
        let finalizers = events.take_finalizers();
        let builder = RequestMetadataBuilder::from_events(&events);
        (finalizers, builder, events)
    }

    fn build_request(
        &self,
        finalizers: Self::Metadata,
        metadata: RequestMetadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        DatabendRequest {
            finalizers,
            data: payload.into_payload(),
            metadata,
        }
    }
}
