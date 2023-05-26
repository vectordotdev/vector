use std::num::NonZeroUsize;

use vector_buffers::EventCount;
use vector_core::{ByteSizeOf, EstimatedJsonEncodedSizeOf};

use vector_common::{json_size::JsonSize, request_metadata::RequestMetadata};

use super::request_builder::EncodeResult;

#[derive(Clone, Default)]
pub struct RequestMetadataBuilder {
    event_count: usize,
    events_byte_size: usize,
    events_estimated_json_encoded_byte_size: JsonSize,
}

impl RequestMetadataBuilder {
    pub fn from_events<E>(events: E) -> Self
    where
        E: ByteSizeOf + EventCount + EstimatedJsonEncodedSizeOf,
    {
        Self {
            event_count: events.event_count(),
            events_byte_size: events.size_of(),
            events_estimated_json_encoded_byte_size: events.estimated_json_encoded_size_of(),
        }
    }

    pub const fn new(
        event_count: usize,
        events_byte_size: usize,
        events_estimated_json_encoded_byte_size: JsonSize,
    ) -> Self {
        Self {
            event_count,
            events_byte_size,
            events_estimated_json_encoded_byte_size,
        }
    }

    pub fn track_event<E>(&mut self, event: E)
    where
        E: ByteSizeOf + EstimatedJsonEncodedSizeOf,
    {
        self.event_count += 1;
        self.events_byte_size += event.size_of();
        self.events_estimated_json_encoded_byte_size += event.estimated_json_encoded_size_of();
    }

    pub fn with_request_size(&self, size: NonZeroUsize) -> RequestMetadata {
        let size = size.get();

        RequestMetadata::new(
            self.event_count,
            self.events_byte_size,
            size,
            size,
            self.events_estimated_json_encoded_byte_size,
        )
    }

    pub fn build<T>(&self, result: &EncodeResult<T>) -> RequestMetadata {
        RequestMetadata::new(
            self.event_count,
            self.events_byte_size,
            result.uncompressed_byte_size,
            result
                .compressed_byte_size
                .unwrap_or(result.uncompressed_byte_size),
            self.events_estimated_json_encoded_byte_size,
        )
    }
}
