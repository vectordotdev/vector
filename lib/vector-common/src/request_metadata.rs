use std::collections::HashMap;
use std::ops::Add;

use crate::internal_event::CountByteSize;

pub type EventCountTags = (Option<String>, Option<String>);

pub trait GetEventCountTags {
    fn get_tags(&self) -> EventCountTags;
}

/// Struct that keeps track of the estimated json size of a given
/// batch of events by source and service.
#[derive(Clone, Debug, Default)]
pub struct RequestCountByteSize {
    sizes: HashMap<EventCountTags, CountByteSize>,
}

impl RequestCountByteSize {
    pub fn sizes(&self) -> &HashMap<EventCountTags, CountByteSize> {
        &self.sizes
    }

    pub fn add_event<E>(&mut self, event: &E, json_size: usize)
    where
        E: GetEventCountTags,
    {
        let size = CountByteSize(1, json_size);
        let tags = event.get_tags();

        match self.sizes.get_mut(&tags) {
            Some(current) => {
                *current += size;
            }
            None => {
                self.sizes.insert(tags, size);
            }
        }
    }
}

impl From<CountByteSize> for RequestCountByteSize {
    fn from(value: CountByteSize) -> Self {
        let mut sizes = HashMap::new();
        sizes.insert((None, None), value);

        Self { sizes }
    }
}

impl<'a> Add<&'a RequestCountByteSize> for RequestCountByteSize {
    type Output = RequestCountByteSize;

    fn add(mut self, other: &'a Self::Output) -> Self::Output {
        for (key, value) in &other.sizes {
            match self.sizes.get_mut(&key) {
                Some(size) => *size += *value,
                None => {
                    self.sizes.insert(key.clone(), *value);
                }
            }
        }

        Self { sizes: self.sizes }
    }
}

/// Metadata for batch requests.
#[derive(Clone, Debug, Default)]
pub struct RequestMetadata {
    /// Number of events represented by this batch request.
    event_count: usize,
    /// Size, in bytes, of the in-memory representation of all events in this batch request.
    events_byte_size: usize,
    /// Size, in bytes, of the estimated JSON-encoded representation of all events in this batch request.
    events_estimated_json_encoded_byte_size: RequestCountByteSize,
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
        events_estimated_json_encoded_byte_size: RequestCountByteSize,
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
    pub fn events_estimated_json_encoded_byte_size(&self) -> &RequestCountByteSize {
        &self.events_estimated_json_encoded_byte_size
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
        let mut metadata_sum = RequestMetadata::new(0, 0, 0, 0, Default::default());

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
                + &other.events_estimated_json_encoded_byte_size,
            request_encoded_size: self.request_encoded_size + other.request_encoded_size,
            request_wire_size: self.request_wire_size + other.request_wire_size,
        }
    }
}

/// Objects implementing this trait have metadata that describes the request.
pub trait MetaDescriptive {
    /// Returns the `RequestMetadata` associated with this object.
    fn get_metadata(&self) -> &RequestMetadata;
}
