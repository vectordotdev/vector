use std::io;

use bytes::Bytes;
use vector_lib::{
    codecs::encoding::Framer,
    event::Event,
    finalization::{EventFinalizers, Finalizable},
    request_metadata::RequestMetadata,
};

use super::service::DatabendRequest;
use crate::{
    codecs::{Encoder, Transformer},
    sinks::util::{
        Compression, RequestBuilder,
        metadata::RequestMetadataBuilder,
        request_builder::{EncodeResult, RequestParts},
    },
};

#[derive(Clone)]
pub(super) struct DatabendRequestBuilder {
    compression: Compression,
    encoder: (Transformer, Encoder<Framer>),
}

impl DatabendRequestBuilder {
    pub(super) const fn new(
        compression: Compression,
        encoder: (Transformer, Encoder<Framer>),
    ) -> Self {
        Self {
            compression,
            encoder,
        }
    }
}

impl RequestBuilder<Vec<Event>> for DatabendRequestBuilder {
    type Metadata = ();
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

    fn split_input(&self, input: Vec<Event>) -> RequestParts<Self::Metadata, Self::Events> {
        let mut events = input;
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
