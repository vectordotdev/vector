use std::num::NonZeroUsize;

use vector_buffers::EventCount;
use vector_core::ByteSizeOf;

use super::request_builder::EncodeResult;

/// Metadata for batch requests.
#[derive(Clone, Debug)]
pub struct RequestMetadata {
    /// Number of events represented by this batch request.
    event_count: usize,
    /// Size, in bytes, of the in-memory representation of all events in this batch request.
    events_byte_size: usize,
    /// Uncompressed size, in bytes, of the encoded events in this batch request.
    request_encoded_size: usize,
    /// On-the-wire size, in bytes, of the batch request itself after compression, etc.
    ///
    /// This is akin to the bytes sent/received over the network, regardless of whether or not compression was used.
    request_wire_size: usize,
}

// TODO: Make this struct the object which emits the actual internal telemetry i.e. events sent, bytes sent, etc.
impl RequestMetadata {
    pub fn builder<E>(events: E) -> RequestMetadataBuilder
    where
        E: ByteSizeOf + EventCount,
    {
        RequestMetadataBuilder::from_events(events)
    }

    pub const fn event_count(&self) -> usize {
        self.event_count
    }

    pub const fn events_byte_size(&self) -> usize {
        self.events_byte_size
    }

    pub const fn request_encoded_size(&self) -> usize {
        self.request_encoded_size
    }

    pub const fn request_wire_size(&self) -> usize {
        self.request_wire_size
    }
}

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
