use bytes::Bytes;
use std::collections::HashMap;
use std::io;
use vector_common::finalization::EventFinalizers;
use vector_common::request_metadata::RequestMetadata;

use crate::sinks::pulsar::encoder::PulsarEncoder;
use crate::sinks::pulsar::sink::PulsarEvent;
use crate::sinks::util::metadata::RequestMetadataBuilder;
use crate::sinks::util::request_builder::EncodeResult;
use crate::sinks::util::{Compression, RequestBuilder};
use crate::{
    event::{Event, Finalizable},
    sinks::pulsar::service::PulsarRequest,
};

#[derive(Clone)]
pub(super) struct PulsarMetadata {
    pub finalizers: EventFinalizers,
    pub key: Option<Bytes>,
    pub properties: Option<HashMap<String, Bytes>>,
    pub timestamp_millis: Option<i64>,
    pub topic: String,
}

pub(super) struct PulsarRequestBuilder {
    pub(super) encoder: PulsarEncoder,
}

impl RequestBuilder<PulsarEvent> for PulsarRequestBuilder {
    type Metadata = PulsarMetadata;
    type Events = Event;
    type Encoder = PulsarEncoder;
    type Payload = Bytes;
    type Request = PulsarRequest;
    type Error = io::Error;

    fn compression(&self) -> Compression {
        // Compression is handled by the pulsar crate through the producer settings.
        Compression::None
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoder
    }

    fn split_input(
        &self,
        mut input: PulsarEvent,
    ) -> (Self::Metadata, RequestMetadataBuilder, Self::Events) {
        let builder = RequestMetadataBuilder::from_events(&input);
        let metadata = PulsarMetadata {
            finalizers: input.event.take_finalizers(),
            key: input.key,
            timestamp_millis: input.timestamp_millis,
            properties: input.properties,
            topic: input.topic,
        };
        (metadata, builder, input.event)
    }

    fn build_request(
        &self,
        metadata: Self::Metadata,
        request_metadata: RequestMetadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        let body = payload.into_payload();
        PulsarRequest {
            body,
            metadata,
            request_metadata,
        }
    }
}
