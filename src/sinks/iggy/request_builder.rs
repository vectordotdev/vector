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

// Holds pre-encode event tags so byte_size can be updated after a successful
// encode (the encoder consumes the event).
struct TagsCapture(TaggedEventsSent);

impl GetEventCountTags for TagsCapture {
    fn get_tags(&self) -> TaggedEventsSent {
        self.0.clone()
    }
}

pub(super) fn request_builder(
    events: Vec<Event>,
    transformer: &Transformer,
    encoder: &mut Encoder<()>,
) -> Option<IggyRequest> {
    let mut byte_size = telemetry().create_request_count_byte_size();
    let mut event_count = 0usize;
    let mut events_byte_size = 0usize;
    let mut uncompressed_byte_size = 0usize;
    let mut finalizers = EventFinalizers::default();

    let payloads: Vec<Bytes> = events
        .into_iter()
        .filter_map(|mut event| {
            transformer.transform(&mut event);
            // Capture telemetry metadata before encoding consumes the event.
            let json_size = event.estimated_json_encoded_size_of();
            let size_of = event.size_of();
            let tags = TagsCapture(event.get_tags());
            let event_finalizers = event.take_finalizers();

            let mut bytes = BytesMut::new();
            match encoder.encode(event, &mut bytes) {
                Ok(()) => {
                    let encoded = bytes.freeze();
                    // Only count events that were successfully encoded.
                    byte_size.add_event(&tags, json_size);
                    event_count += 1;
                    events_byte_size += size_of;
                    uncompressed_byte_size += encoded.len();
                    finalizers.merge(event_finalizers);
                    Some(encoded)
                }
                Err(_) => {
                    event_finalizers.update_status(EventStatus::Errored);
                    None
                }
            }
        })
        .collect();

    // Every event in the batch failed encoding; their finalizers were
    // already marked Errored individually above. Drop the request rather
    // than dispatching an empty one, which would still emit "Delivered"
    // telemetry for the original event count even though nothing was sent.
    if payloads.is_empty() {
        return None;
    }

    let builder = RequestMetadataBuilder::new(
        event_count,
        events_byte_size,
        telemetry().create_request_count_byte_size(),
    );
    let encoded = EncodeResult {
        payload: payloads,
        uncompressed_byte_size,
        transformed_json_size: byte_size,
        compressed_byte_size: None,
    };
    let metadata = builder.build(&encoded);

    Some(IggyRequest {
        payloads: encoded.into_payload(),
        finalizers,
        metadata,
    })
}
