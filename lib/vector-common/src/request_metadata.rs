use std::ops::Add;

use crate::json_size::JsonSize;

/// Metadata for batch requests.
#[derive(Clone, Copy, Debug, Default)]
pub struct RequestMetadata {
    /// Number of events represented by this batch request.
    event_count: usize,
    /// Size, in bytes, of the in-memory representation of all events in this batch request.
    events_byte_size: usize,
    /// Size, in bytes, of the estimated JSON-encoded representation of all events in this batch request.
    events_estimated_json_encoded_byte_size: JsonSize,
    /// Uncompressed size, in bytes, of the encoded events in this batch request.
    request_encoded_size: usize,
    /// On-the-wire size, in bytes, of the batch request itself after compression, etc.
    ///
    /// This is akin to the bytes sent/received over the network, regardless of whether or not compression was used.
    request_wire_size: usize,
}

// TODO: Make this struct the object which emits the actual internal telemetry i.e. events sent, bytes sent, etc.
impl RequestMetadata {
    #[must_use]
    pub fn new(
        event_count: usize,
        events_byte_size: usize,
        request_encoded_size: usize,
        request_wire_size: usize,
        events_estimated_json_encoded_byte_size: JsonSize,
    ) -> Self {
        Self {
            event_count,
            events_byte_size,
            events_estimated_json_encoded_byte_size,
            request_encoded_size,
            request_wire_size,
        }
    }

    #[must_use]
    pub const fn event_count(&self) -> usize {
        self.event_count
    }

    #[must_use]
    pub const fn events_byte_size(&self) -> usize {
        self.events_byte_size
    }

    #[must_use]
    pub const fn events_estimated_json_encoded_byte_size(&self) -> JsonSize {
        self.events_estimated_json_encoded_byte_size
    }

    #[must_use]
    pub const fn request_encoded_size(&self) -> usize {
        self.request_encoded_size
    }

    #[must_use]
    pub const fn request_wire_size(&self) -> usize {
        self.request_wire_size
    }

    /// Constructs a `RequestMetadata` by summation of the "batch" of `RequestMetadata` provided.
    #[must_use]
    pub fn from_batch<T: IntoIterator<Item = RequestMetadata>>(metadata_iter: T) -> Self {
        let mut metadata_sum = RequestMetadata::new(0, 0, 0, 0, JsonSize::zero());

        for metadata in metadata_iter {
            metadata_sum = metadata_sum + &metadata;
        }
        metadata_sum
    }
}

impl<'a> Add<&'a RequestMetadata> for RequestMetadata {
    type Output = RequestMetadata;

    /// Adds the other `RequestMetadata` to this one.
    fn add(self, other: &'a Self::Output) -> Self::Output {
        Self::Output {
            event_count: self.event_count + other.event_count,
            events_byte_size: self.events_byte_size + other.events_byte_size,
            events_estimated_json_encoded_byte_size: self.events_estimated_json_encoded_byte_size
                + other.events_estimated_json_encoded_byte_size,
            request_encoded_size: self.request_encoded_size + other.request_encoded_size,
            request_wire_size: self.request_wire_size + other.request_wire_size,
        }
    }
}

/// Objects implementing this trait have metadata that describes the request.
pub trait MetaDescriptive {
    /// Returns the `RequestMetadata` associated with this object.
    fn get_metadata(&self) -> RequestMetadata;
}
