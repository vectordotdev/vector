use bytes::{Bytes, BytesMut};
use iggy::prelude::MAX_PAYLOAD_SIZE;
use tokio_util::codec::Encoder as _;
use vector_lib::{config::telemetry, internal_event::emit};

use crate::{internal_events::IggyInvalidPayloadError, sinks::prelude::*};

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

// Keeps pre-encode tags for byte-size telemetry; encoding consumes the event.
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

    let mut buf = BytesMut::with_capacity(1024);
    let mut payloads = Vec::with_capacity(events.len());
    for mut event in events {
        transformer.transform(&mut event);
        let json_size = event.estimated_json_encoded_size_of();
        let size_of = event.size_of();
        let tags = TagsCapture(event.get_tags());
        let event_finalizers = event.take_finalizers();

        match encoder.encode(event, &mut buf) {
            Ok(()) => {
                let frame = buf.split().freeze();
                // The SDK rejects empty and oversized payloads, which would
                // fail the whole request and error its valid siblings.
                if frame.is_empty() || frame.len() > MAX_PAYLOAD_SIZE as usize {
                    emit(IggyInvalidPayloadError {
                        payload_len: frame.len(),
                    });
                    event_finalizers.update_status(EventStatus::Errored);
                    continue;
                }
                byte_size.add_event(&tags, json_size);
                event_count += 1;
                events_byte_size += size_of;
                uncompressed_byte_size += frame.len();
                finalizers.merge(event_finalizers);
                payloads.push(frame);
            }
            Err(_) => {
                event_finalizers.update_status(EventStatus::Errored);
            }
        }
    }

    // All events failed encoding and were marked Errored above. Drop the
    // request instead of emitting "Delivered" for an empty send.
    if payloads.is_empty() {
        return None;
    }

    let builder = RequestMetadataBuilder::new(
        event_count,
        events_byte_size,
        telemetry().create_request_count_byte_size(),
    );
    let result = EncodeResult {
        payload: payloads,
        uncompressed_byte_size,
        transformed_json_size: byte_size,
        compressed_byte_size: None,
    };
    let metadata = builder.build(&result);

    Some(IggyRequest {
        payloads: result.into_payload(),
        finalizers,
        metadata,
    })
}
