use std::io;

use async_nats::HeaderMap;
use bytes::{Bytes, BytesMut};
use tokio_util::codec::Encoder as _;
use vector_lib::config::telemetry;

use crate::sinks::prelude::*;

use super::{config::NatsHeaderConfig, sink::NatsEvent};

pub(super) struct NatsEncoder {
    pub(super) transformer: Transformer,
    pub(super) encoder: Encoder<()>,
}

impl encoding::Encoder<Event> for NatsEncoder {
    fn encode_input(
        &self,
        mut input: Event,
        writer: &mut dyn io::Write,
    ) -> io::Result<(usize, GroupedCountByteSize)> {
        let mut body = BytesMut::new();
        self.transformer.transform(&mut input);

        let mut byte_size = telemetry().create_request_count_byte_size();
        byte_size.add_event(&input, input.estimated_json_encoded_size_of());

        let mut encoder = self.encoder.clone();
        encoder
            .encode(input, &mut body)
            .map_err(|_| io::Error::other("unable to encode"))?;

        let body = body.freeze();
        write_all(writer, 1, body.as_ref())?;

        Ok((body.len(), byte_size))
    }
}

pub(super) struct NatsMetadata {
    subject: String,
    headers: HeaderMap,
    finalizers: EventFinalizers,
}

pub(super) struct NatsRequestBuilder {
    pub(super) encoder: NatsEncoder,
    pub(super) headers: Option<NatsHeaderConfig>,
}

#[derive(Clone)]
pub(super) struct NatsRequest {
    pub(super) bytes: Bytes,
    pub(super) subject: String,
    pub(super) headers: HeaderMap,
    finalizers: EventFinalizers,
    pub(super) metadata: RequestMetadata,
}

impl Finalizable for NatsRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.finalizers)
    }
}

impl MetaDescriptive for NatsRequest {
    fn get_metadata(&self) -> &RequestMetadata {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut RequestMetadata {
        &mut self.metadata
    }
}

impl RequestBuilder<NatsEvent> for NatsRequestBuilder {
    type Metadata = NatsMetadata;
    type Events = Event;
    type Encoder = NatsEncoder;
    type Payload = Bytes;
    type Request = NatsRequest;
    type Error = io::Error;

    fn compression(&self) -> Compression {
        Compression::None
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoder
    }

    fn split_input(
        &self,
        mut input: NatsEvent,
    ) -> (Self::Metadata, RequestMetadataBuilder, Self::Events) {
        let builder = RequestMetadataBuilder::from_event(&input.event);
        let headers = self
            .headers
            .as_ref()
            .map(|config| config.build_headers(&input.event))
            .unwrap_or_default();

        let metadata = NatsMetadata {
            subject: input.subject,
            headers,
            finalizers: input.event.take_finalizers(),
        };

        (metadata, builder, input.event)
    }

    fn build_request(
        &self,
        nats_metadata: Self::Metadata,
        metadata: RequestMetadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        let body = payload.into_payload();
        NatsRequest {
            bytes: body,
            subject: nats_metadata.subject,
            headers: nats_metadata.headers,
            finalizers: nats_metadata.finalizers,
            metadata,
        }
    }
}
