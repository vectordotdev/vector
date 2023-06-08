use std::collections::HashMap;
use std::ops::Add;

use crate::internal_event::{CountByteSize, OptionalTag};
use crate::json_size::JsonSize;

/// (Source, Service)
pub type EventCountTags = (OptionalTag, OptionalTag);

pub trait GetEventCountTags {
    fn get_tags(&self) -> EventCountTags;
}

/// Struct that keeps track of the estimated json size of a given
/// batch of events by source and service.
#[derive(Clone, Debug)]
pub enum RequestCountByteSize {
    Tagged {
        sizes: HashMap<EventCountTags, CountByteSize>,
    },
    Untagged {
        size: CountByteSize,
    },
}

impl Default for RequestCountByteSize {
    fn default() -> Self {
        Self::Untagged {
            size: CountByteSize(0, JsonSize::zero()),
        }
    }
}

impl RequestCountByteSize {
    pub fn new_tagged() -> Self {
        Self::Tagged {
            sizes: Default::default(),
        }
    }

    pub fn new_untagged() -> Self {
        Self::Untagged {
            size: CountByteSize(0, JsonSize::zero()),
        }
    }

    #[must_use]
    pub fn sizes(&self) -> Option<&HashMap<EventCountTags, CountByteSize>> {
        match self {
            RequestCountByteSize::Tagged { sizes } => Some(sizes),
            RequestCountByteSize::Untagged { .. } => None,
        }
    }

    #[must_use]
    pub fn size(&self) -> Option<CountByteSize> {
        match self {
            RequestCountByteSize::Tagged { .. } => None,
            RequestCountByteSize::Untagged { size } => Some(*size),
        }
    }

    pub fn add_event<E>(&mut self, event: &E, json_size: JsonSize)
    where
        E: GetEventCountTags,
    {
        match self {
            RequestCountByteSize::Tagged { sizes } => {
                let size = CountByteSize(1, json_size);
                let tags = event.get_tags();

                match sizes.get_mut(&tags) {
                    Some(current) => {
                        *current += size;
                    }
                    None => {
                        sizes.insert(tags, size);
                    }
                }
            }
            RequestCountByteSize::Untagged { size } => {
                *size += CountByteSize(1, json_size);
            }
        }
    }
}

impl From<CountByteSize> for RequestCountByteSize {
    fn from(value: CountByteSize) -> Self {
        Self::Untagged { size: value }
    }
}

impl<'a> Add<&'a RequestCountByteSize> for RequestCountByteSize {
    type Output = RequestCountByteSize;

    fn add(self, other: &'a Self::Output) -> Self::Output {
        match (self, other) {
            (
                RequestCountByteSize::Tagged { sizes: mut sizesa },
                RequestCountByteSize::Tagged { sizes: sizesb },
            ) => {
                for (key, value) in sizesb {
                    match sizesa.get_mut(key) {
                        Some(size) => *size += *value,
                        None => {
                            sizesa.insert(key.clone(), *value);
                        }
                    }
                }

                Self::Tagged { sizes: sizesa }
            }
            // (
            //     RequestCountByteSize::Tagged { mut sizes },
            //     RequestCountByteSize::Untagged { size },
            // ) => {
            //     // TODO - work this out
            //     panic!("DONT DO THIS!!!");
            //     // match sizes.get_mut(&(None, None)) {
            //     //     Some(sizea) => *sizea += *size,
            //     //     None => {
            //     //         sizes.insert((None, None), *size);
            //     //     }
            //     // }

            //     // Self::Tagged { sizes }
            // }
            // (RequestCountByteSize::Untagged { size }, RequestCountByteSize::Tagged { sizes }) => {
            //     panic!("DONT DO THIS!!!");
            //     // let mut sizes = sizes.clone();
            //     // match sizes.get_mut(&(None, None)) {
            //     //     Some(sizea) => *sizea += size,
            //     //     None => {
            //     //         sizes.insert((None, None), size);
            //     //     }
            //     // }

            //     // Self::Tagged { sizes }
            // }
            (
                RequestCountByteSize::Untagged { size: sizea },
                RequestCountByteSize::Untagged { size: sizeb },
            ) => RequestCountByteSize::Untagged {
                size: sizea + *sizeb,
            },
            _ => panic!("DONT"),
        }
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

    /// Consumes the object and returns the byte size of the request grouped by
    /// the tags (source and service).
    #[must_use]
    pub fn into_events_estimated_json_encoded_byte_size(self) -> RequestCountByteSize {
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
        let mut metadata_sum = RequestMetadata::new(0, 0, 0, 0, RequestCountByteSize::default());

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

    /// Returns the owned `RequestMetadata`.
    /// TODO Remove the default implementation because this is a
    /// terrible way to do it.
    fn take_metadata(&mut self) -> RequestMetadata {
        self.get_metadata().clone()
    }
}
