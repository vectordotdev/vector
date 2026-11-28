use std::{collections::HashMap, io};

use bytes::Bytes;

use crate::{
    event::KeyString,
    sinks::{
        prelude::*,
        pulsar::{encoder::PulsarEncoder, service::PulsarRequest, sink::PulsarEvent},
    },
};

#[derive(Clone)]
pub(super) struct PulsarMetadata {
    pub key: Option<Bytes>,
    pub properties: Option<HashMap<KeyString, Bytes>>,
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

    fn split_input(&self, mut input: PulsarEvent) -> RequestParts<Self::Metadata, Self::Events> {
        let builder = RequestMetadataBuilder::from_event(&input.event);
        let finalizers = input.event.take_finalizers();
        let metadata = PulsarMetadata {
            key: input.key,
            timestamp_millis: input.timestamp_millis,
            properties: input.properties,
            topic: input.topic,
        };
        RequestParts {
            metadata,
            request_metadata_builder: builder,
            events: input.event,
            finalizers,
        }
    }

    fn build_request(
        &self,
        metadata: Self::Metadata,
        finalizers: EventFinalizers,
        request_metadata: RequestMetadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        let body = payload.into_payload();
        PulsarRequest {
            finalizers,
            body,
            metadata,
            request_metadata,
        }
    }
}
