use std::io;

use bytes::{Bytes, BytesMut};
use tokio_util::codec::Encoder as _;

use crate::sinks::prelude::*;

use super::{service::MqttRequest, sink::MqttEvent};

pub(super) struct MqttMetadata {
    topic: String,
    finalizers: EventFinalizers,
}

pub(super) struct MqttEncoder {
    pub(super) encoder: crate::codecs::Encoder<()>,
    pub(super) transformer: crate::codecs::Transformer,
}

impl encoding::Encoder<Event> for MqttEncoder {
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
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "unable to encode"))?;

        let body = body.freeze();
        write_all(writer, 1, body.as_ref())?;

        Ok((body.len(), byte_size))
    }
}

pub(super) struct MqttRequestBuilder {
    pub(super) encoder: MqttEncoder,
}

impl RequestBuilder<MqttEvent> for MqttRequestBuilder {
    type Metadata = MqttMetadata;
    type Events = Event;
    type Encoder = MqttEncoder;
    type Payload = Bytes;
    type Request = MqttRequest;
    type Error = io::Error;

    fn compression(&self) -> Compression {
        Compression::None
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoder
    }

    fn split_input(
        &self,
        mut input: MqttEvent,
    ) -> (Self::Metadata, RequestMetadataBuilder, Self::Events) {
        let builder = RequestMetadataBuilder::from_event(&input.event);

        let metadata = MqttMetadata {
            topic: input.topic,
            finalizers: input.event.take_finalizers(),
        };

        (metadata, builder, input.event)
    }

    fn build_request(
        &self,
        mqtt_metadata: Self::Metadata,
        metadata: RequestMetadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        let body = payload.into_payload();
        MqttRequest {
            body,
            topic: mqtt_metadata.topic,
            finalizers: mqtt_metadata.finalizers,
            metadata,
        }
    }
}
