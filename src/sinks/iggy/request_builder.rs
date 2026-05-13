use bytes::{Bytes, BytesMut};
use tokio_util::codec::Encoder as _;
use vector_lib::config::telemetry;

use crate::sinks::prelude::*;

#[derive(Clone)]
pub(super) struct IggyRequest {
    pub(super) payloads: Vec<Bytes>,
    finalizers: EventFinalizers,
    pub(super) metadata: RequestMetadata,
}

impl Finalizable for IggyRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.finalizers)
    }
}

impl MetaDescriptive for IggyRequest {
    fn get_metadata(&self) -> &RequestMetadata {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut RequestMetadata {
        &mut self.metadata
    }
}

fn encode_event(
    mut event: Event,
    transformer: &Transformer,
    encoder: &mut Encoder<()>,
    byte_size: &mut GroupedCountByteSize,
) -> Option<Bytes> {
    transformer.transform(&mut event);
    byte_size.add_event(&event, event.estimated_json_encoded_size_of());

    let mut bytes = BytesMut::new();
    encoder.encode(event, &mut bytes).ok()?;
    Some(bytes.freeze())
}

pub(super) fn request_builder(
    events: Vec<Event>,
    transformer: &Transformer,
    encoder: &mut Encoder<()>,
) -> IggyRequest {
    let builder = RequestMetadataBuilder::from_events(&events);

    let mut byte_size = telemetry().create_request_count_byte_size();
    let mut uncompressed_byte_size = 0usize;
    let mut finalizers = EventFinalizers::default();
    let payloads: Vec<Bytes> = events
        .into_iter()
        .filter_map(|mut event| {
            let event_finalizers = event.take_finalizers();
            match encode_event(event, transformer, encoder, &mut byte_size) {
                Some(bytes) => {
                    uncompressed_byte_size += bytes.len();
                    finalizers.merge(event_finalizers);
                    Some(bytes)
                }
                None => {
                    event_finalizers.update_status(EventStatus::Errored);
                    None
                }
            }
        })
        .collect();
    let encoded = EncodeResult {
        payload: payloads,
        uncompressed_byte_size,
        transformed_json_size: byte_size,
        compressed_byte_size: None,
    };
    let metadata = builder.build(&encoded);

    IggyRequest {
        payloads: encoded.into_payload(),
        finalizers,
        metadata,
    }
}
