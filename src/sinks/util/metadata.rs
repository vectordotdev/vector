use std::num::NonZeroUsize;

use vector_buffers::EventCount;
use vector_common::metadata::RequestMetadata;
use vector_core::ByteSizeOf;

use super::request_builder::EncodeResult;

pub struct RequestMetadataBuilder {
    event_count: usize,
    events_byte_size: usize,
}

impl RequestMetadataBuilder {
    pub fn from_events<E>(events: E) -> Self
    where
        E: ByteSizeOf + EventCount,
    {
        Self {
            event_count: events.event_count(),
            events_byte_size: events.size_of(),
        }
    }

    pub const fn with_request_size(self, size: NonZeroUsize) -> RequestMetadata {
        RequestMetadata {
            event_count: self.event_count,
            events_byte_size: self.events_byte_size,
            request_encoded_size: size.get(),
            request_wire_size: size.get(),
        }
    }

    pub fn build<T>(self, result: &EncodeResult<T>) -> RequestMetadata {
        RequestMetadata {
            event_count: self.event_count,
            events_byte_size: self.events_byte_size,
            request_encoded_size: result.uncompressed_byte_size,
            request_wire_size: result
                .compressed_byte_size
                .unwrap_or(result.uncompressed_byte_size),
        }
    }
}
