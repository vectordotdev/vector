use std::ops::Add;
use std::{collections::HashMap, sync::Arc};

use crate::{
    config::ComponentKey,
    internal_event::{
        CountByteSize, InternalEventHandle, OptionalTag, RegisterTaggedInternalEvent,
        RegisteredEventCache,
    },
    json_size::JsonSize,
};

/// Tags that are used to group the events within a batch for emitting telemetry.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EventCountTags {
    pub source: OptionalTag<Arc<ComponentKey>>,
    pub service: OptionalTag<String>,
}

impl EventCountTags {
    #[must_use]
    pub fn new_empty() -> Self {
        Self {
            source: OptionalTag::Specified(None),
            service: OptionalTag::Specified(None),
        }
    }
}

/// Must be implemented by events to get the tags that will be attached to
/// the `component_sent_event_*` emitted metrics.
pub trait GetEventCountTags {
    fn get_tags(&self) -> EventCountTags;
}

/// Keeps track of the estimated json size of a given batch of events by
/// source and service.
#[derive(Clone, Debug)]
pub enum GroupedCountByteSize {
    /// When we need to keep track of the events by certain tags we use this
    /// variant.
    Tagged {
        sizes: HashMap<EventCountTags, CountByteSize>,
    },
    /// If we don't need to track the events by certain tags we can use
    /// this variant to avoid allocating a `HashMap`,
    Untagged { size: CountByteSize },
}

impl Default for GroupedCountByteSize {
    fn default() -> Self {
        Self::Untagged {
            size: CountByteSize(0, JsonSize::zero()),
        }
    }
}

impl GroupedCountByteSize {
    /// Creates a new Tagged variant for when we need to track events by
    /// certain tags.
    #[must_use]
    pub fn new_tagged() -> Self {
        Self::Tagged {
            sizes: HashMap::new(),
        }
    }

    /// Creates a new Tagged variant for when we do not need to track events by
    /// tags.
    #[must_use]
    pub fn new_untagged() -> Self {
        Self::Untagged {
            size: CountByteSize(0, JsonSize::zero()),
        }
    }

    /// Returns a `HashMap` of tags => event counts for when we are tracking by tags.
    /// Returns `None` if we are not tracking by tags.
    #[must_use]
    #[cfg(test)]
    pub fn sizes(&self) -> Option<&HashMap<EventCountTags, CountByteSize>> {
        match self {
            Self::Tagged { sizes } => Some(sizes),
            Self::Untagged { .. } => None,
        }
    }

    /// Returns a single count for when we are not tracking by tags.
    #[must_use]
    #[cfg(test)]
    fn size(&self) -> Option<CountByteSize> {
        match self {
            Self::Tagged { .. } => None,
            Self::Untagged { size } => Some(*size),
        }
    }

    /// Adds the given estimated json size of the event to current count.
    pub fn add_event<E>(&mut self, event: &E, json_size: JsonSize)
    where
        E: GetEventCountTags,
    {
        match self {
            Self::Tagged { sizes } => {
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
            Self::Untagged { size } => {
                *size += CountByteSize(1, json_size);
            }
        }
    }

    /// Emits our counts to a `RegisteredEvent` cached event.
    pub fn emit_event<T, H>(&self, event_cache: &RegisteredEventCache<T>)
    where
        T: RegisterTaggedInternalEvent<Tags = EventCountTags, Handle = H>,
        H: InternalEventHandle<Data = CountByteSize>,
    {
        match self {
            GroupedCountByteSize::Tagged { sizes } => {
                for (tags, size) in sizes {
                    event_cache.emit(tags, *size);
                }
            }
            GroupedCountByteSize::Untagged { size } => {
                event_cache.emit(&EventCountTags::new_empty(), *size);
            }
        }
    }
}

impl From<CountByteSize> for GroupedCountByteSize {
    fn from(value: CountByteSize) -> Self {
        Self::Untagged { size: value }
    }
}

impl<'a> Add<&'a GroupedCountByteSize> for GroupedCountByteSize {
    type Output = GroupedCountByteSize;

    fn add(self, other: &'a Self::Output) -> Self::Output {
        match (self, other) {
            (Self::Tagged { sizes: mut us }, Self::Tagged { sizes: them }) => {
                for (key, value) in them {
                    match us.get_mut(key) {
                        Some(size) => *size += *value,
                        None => {
                            us.insert(key.clone(), *value);
                        }
                    }
                }

                Self::Tagged { sizes: us }
            }

            (Self::Untagged { size: us }, Self::Untagged { size: them }) => {
                Self::Untagged { size: us + *them }
            }

            // The following two scenarios shouldn't really occur in practice, but are provided for completeness.
            (Self::Tagged { mut sizes }, Self::Untagged { size }) => {
                match sizes.get_mut(&EventCountTags::new_empty()) {
                    Some(empty_size) => *empty_size += *size,
                    None => {
                        sizes.insert(EventCountTags::new_empty(), *size);
                    }
                }

                Self::Tagged { sizes }
            }
            (Self::Untagged { size }, Self::Tagged { sizes }) => {
                let mut sizes = sizes.clone();
                match sizes.get_mut(&EventCountTags::new_empty()) {
                    Some(empty_size) => *empty_size += size,
                    None => {
                        sizes.insert(EventCountTags::new_empty(), size);
                    }
                }

                Self::Tagged { sizes }
            }
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
    events_estimated_json_encoded_byte_size: GroupedCountByteSize,
    /// Uncompressed size, in bytes, of the encoded events in this batch request.
    request_encoded_size: usize,
    /// On-the-wire size, in bytes, of the batch request itself after compression, etc.
    ///
    /// This is akin to the bytes sent/received over the network, regardless of whether or not compression was used.
    request_wire_size: usize,
}

impl RequestMetadata {
    #[must_use]
    pub fn new(
        event_count: usize,
        events_byte_size: usize,
        request_encoded_size: usize,
        request_wire_size: usize,
        events_estimated_json_encoded_byte_size: GroupedCountByteSize,
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
    pub fn events_estimated_json_encoded_byte_size(&self) -> &GroupedCountByteSize {
        &self.events_estimated_json_encoded_byte_size
    }

    /// Consumes the object and returns the byte size of the request grouped by
    /// the tags (source and service).
    #[must_use]
    pub fn into_events_estimated_json_encoded_byte_size(self) -> GroupedCountByteSize {
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
        let mut metadata_sum = RequestMetadata::new(0, 0, 0, 0, GroupedCountByteSize::default());

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

    // Returns a mutable reference to the `RequestMetadata` associated with this object.
    fn metadata_mut(&mut self) -> &mut RequestMetadata;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct DummyEvent {
        source: OptionalTag<Arc<ComponentKey>>,
        service: OptionalTag<String>,
    }

    impl GetEventCountTags for DummyEvent {
        fn get_tags(&self) -> EventCountTags {
            EventCountTags {
                source: self.source.clone(),
                service: self.service.clone(),
            }
        }
    }

    #[test]
    fn add_request_count_bytesize_event_untagged() {
        let mut bytesize = GroupedCountByteSize::new_untagged();
        let event = DummyEvent {
            source: Some(Arc::new(ComponentKey::from("carrot"))).into(),
            service: Some("cabbage".to_string()).into(),
        };

        bytesize.add_event(&event, JsonSize::new(42));

        let event = DummyEvent {
            source: Some(Arc::new(ComponentKey::from("pea"))).into(),
            service: Some("potato".to_string()).into(),
        };

        bytesize.add_event(&event, JsonSize::new(36));

        assert_eq!(Some(CountByteSize(2, JsonSize::new(78))), bytesize.size());
        assert_eq!(None, bytesize.sizes());
    }

    #[test]
    fn add_request_count_bytesize_event_tagged() {
        let mut bytesize = GroupedCountByteSize::new_tagged();
        let event = DummyEvent {
            source: OptionalTag::Ignored,
            service: Some("cabbage".to_string()).into(),
        };

        bytesize.add_event(&event, JsonSize::new(42));

        let event = DummyEvent {
            source: OptionalTag::Ignored,
            service: Some("cabbage".to_string()).into(),
        };

        bytesize.add_event(&event, JsonSize::new(36));

        let event = DummyEvent {
            source: OptionalTag::Ignored,
            service: Some("tomato".to_string()).into(),
        };

        bytesize.add_event(&event, JsonSize::new(23));

        assert_eq!(None, bytesize.size());
        let mut sizes = bytesize
            .sizes()
            .unwrap()
            .clone()
            .into_iter()
            .collect::<Vec<_>>();
        sizes.sort();

        assert_eq!(
            vec![
                (
                    EventCountTags {
                        source: OptionalTag::Ignored,
                        service: Some("cabbage".to_string()).into()
                    },
                    CountByteSize(2, JsonSize::new(78))
                ),
                (
                    EventCountTags {
                        source: OptionalTag::Ignored,
                        service: Some("tomato".to_string()).into()
                    },
                    CountByteSize(1, JsonSize::new(23))
                ),
            ],
            sizes
        );
    }
}
