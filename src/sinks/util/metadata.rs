use std::num::NonZeroUsize;

use vector_buffers::EventCount;
use vector_common::request_metadata::RequestMetadata;
use vector_core::ByteSizeOf;

use super::request_builder::EncodeResult;

#[derive(Clone, Default, Debug)]
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

    pub const fn new(event_count: usize, events_byte_size: usize) -> Self {
        Self {
            event_count,
            events_byte_size,
        }
    }

    pub fn increment(&mut self, event_count: usize, events_byte_size: usize) {
        self.event_count += event_count;
        self.events_byte_size += events_byte_size;
    }

    pub fn with_request_size(&self, size: NonZeroUsize) -> RequestMetadata {
        let size = size.get();

        RequestMetadata::new(self.event_count, self.events_byte_size, size, size)
    }

    pub fn build<T>(&self, result: &EncodeResult<T>) -> RequestMetadata {
        RequestMetadata::new(
            self.event_count,
            self.events_byte_size,
            result.uncompressed_byte_size,
            result
                .compressed_byte_size
                .unwrap_or(result.uncompressed_byte_size),
        )
    }
}
