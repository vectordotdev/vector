use std::{
    collections::HashMap,
    ops::{Add, AddAssign},
    sync::{Arc, Mutex},
};

use crate::{
    internal_event::{
        CountByteSize, InternalEventHandle, RegisterTaggedInternalEvent, RegisteredEventCache,
        TaggedEventsSent,
    },
    json_size::JsonSize,
};

/// Must be implemented by events to get the tags that will be attached to
/// the `component_sent_event_*` emitted metrics.
pub trait GetEventCountTags {
    fn get_tags(&self) -> TaggedEventsSent;
}

/// Keeps track of the estimated json size of a given batch of events by
/// source and service.
#[derive(Clone, Debug)]
pub enum GroupedCountByteSize {
    /// When we need to keep track of the events by certain tags we use this
    /// variant.
    Tagged {
        sizes: HashMap<TaggedEventsSent, CountByteSize>,
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
    #[cfg(any(test, feature = "test"))]
    pub fn sizes(&self) -> Option<&HashMap<TaggedEventsSent, CountByteSize>> {
        match self {
            Self::Tagged { sizes } => Some(sizes),
            Self::Untagged { .. } => None,
        }
    }

    /// Returns a single count for when we are not tracking by tags.
    #[must_use]
    #[cfg(any(test, feature = "test"))]
    pub fn size(&self) -> Option<CountByteSize> {
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
    pub fn emit_event<T, H>(&self, event_cache: &RegisteredEventCache<(), T>)
    where
        T: RegisterTaggedInternalEvent<Tags = TaggedEventsSent, Fixed = (), Handle = H>,
        H: InternalEventHandle<Data = CountByteSize>,
    {
        match self {
            GroupedCountByteSize::Tagged { sizes } => {
                for (tags, size) in sizes {
                    event_cache.emit(tags, *size);
                }
            }
            GroupedCountByteSize::Untagged { size } => {
                event_cache.emit(&TaggedEventsSent::new_unspecified(), *size);
            }
        }
    }

    /// Returns `true` if we are the `Tagged` variant - keeping track of the byte sizes
    /// grouped by their relevant tags.
    #[must_use]
    pub fn is_tagged(&self) -> bool {
        match self {
            GroupedCountByteSize::Tagged { .. } => true,
            GroupedCountByteSize::Untagged { .. } => false,
        }
    }

    /// Returns `true` if we are the `Untagged` variant - keeping a single count for all events.
    #[must_use]
    pub fn is_untagged(&self) -> bool {
        !self.is_tagged()
    }
}

impl From<CountByteSize> for GroupedCountByteSize {
    fn from(value: CountByteSize) -> Self {
        Self::Untagged { size: value }
    }
}

impl AddAssign for GroupedCountByteSize {
    fn add_assign(&mut self, mut rhs: Self) {
        if self.is_untagged() && rhs.is_tagged() {
            // First handle the case where we are untagged and assigning to a tagged value.
            // We need to change `self` and so need to ensure our match doesn't take ownership of the object.
            *self = match (&self, &mut rhs) {
                (Self::Untagged { size }, Self::Tagged { sizes }) => {
                    let mut sizes = std::mem::take(sizes);
                    match sizes.get_mut(&TaggedEventsSent::new_empty()) {
                        Some(empty_size) => *empty_size += *size,
                        None => {
                            sizes.insert(TaggedEventsSent::new_empty(), *size);
                        }
                    }

                    Self::Tagged { sizes }
                }
                _ => {
                    unreachable!()
                }
            };

            return;
        }

        // For these cases, we know we won't have to change `self` so the match can take ownership.
        match (self, rhs) {
            (&mut Self::Tagged { sizes: ref mut lhs }, Self::Tagged { sizes: rhs }) => {
                for (key, value) in rhs {
                    match lhs.get_mut(&key) {
                        Some(size) => *size += value,
                        None => {
                            lhs.insert(key.clone(), value);
                        }
                    }
                }
            }

            (Self::Untagged { size: lhs }, Self::Untagged { size: rhs }) => {
                *lhs = *lhs + rhs;
            }

            (Self::Tagged { sizes }, Self::Untagged { size }) => {
                match sizes.get_mut(&TaggedEventsSent::new_empty()) {
                    Some(empty_size) => *empty_size += size,
                    None => {
                        sizes.insert(TaggedEventsSent::new_empty(), size);
                    }
                }
            }
            (Self::Untagged { .. }, Self::Tagged { .. }) => unreachable!(),
        }
    }
}

impl<'a> Add<&'a GroupedCountByteSize> for GroupedCountByteSize {
    type Output = GroupedCountByteSize;

    fn add(self, other: &'a Self::Output) -> Self::Output {
        match (self, other) {
            (Self::Tagged { sizes: mut lhs }, Self::Tagged { sizes: rhs }) => {
                for (key, value) in rhs {
                    match lhs.get_mut(key) {
                        Some(size) => *size += *value,
                        None => {
                            lhs.insert(key.clone(), *value);
                        }
                    }
                }

                Self::Tagged { sizes: lhs }
            }

            (Self::Untagged { size: lhs }, Self::Untagged { size: rhs }) => {
                Self::Untagged { size: lhs + *rhs }
            }

            // The following two scenarios shouldn't really occur in practice, but are provided for completeness.
            (Self::Tagged { mut sizes }, Self::Untagged { size }) => {
                match sizes.get_mut(&TaggedEventsSent::new_empty()) {
                    Some(empty_size) => *empty_size += *size,
                    None => {
                        sizes.insert(TaggedEventsSent::new_empty(), *size);
                    }
                }

                Self::Tagged { sizes }
            }
            (Self::Untagged { size }, Self::Tagged { sizes }) => {
                let mut sizes = sizes.clone();
                match sizes.get_mut(&TaggedEventsSent::new_empty()) {
                    Some(empty_size) => *empty_size += size,
                    None => {
                        sizes.insert(TaggedEventsSent::new_empty(), size);
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

/// Delivery progress shared by all attempts of one logical request.
/// Each original event is counted at most once, even when a retry sends it again.
#[derive(Clone, Debug)]
pub struct RequestDelivery(Arc<Mutex<DeliveryProgress>>);

#[derive(Debug)]
struct DeliveryProgress {
    delivered: Vec<bool>,
    events_sent: Option<GroupedCountByteSize>,
    remaining: usize,
}

impl RequestDelivery {
    #[must_use]
    pub fn new(event_count: usize) -> Self {
        Self(Arc::new(Mutex::new(DeliveryProgress {
            delivered: vec![false; event_count],
            events_sent: None,
            remaining: event_count,
        })))
    }

    /// Records a confirmed delivery using its index in the original request.
    ///
    /// # Panics
    /// Panics if the index is outside the original request or the lock is poisoned.
    pub fn record(&self, index: usize, size: GroupedCountByteSize) {
        let mut progress = self.0.lock().expect("Request delivery lock poisoned");
        if !progress.delivered[index] {
            progress.delivered[index] = true;
            match &mut progress.events_sent {
                Some(sent) => *sent += size,
                None => progress.events_sent = Some(size),
            }
            progress.remaining -= 1;
        }
    }

    /// Returns confirmed deliveries, if any, and the number of events still unconfirmed.
    ///
    /// # Panics
    /// Panics if the lock is poisoned.
    #[must_use]
    pub fn snapshot(&self) -> (Option<GroupedCountByteSize>, usize) {
        let progress = self.0.lock().expect("Request delivery lock poisoned");
        (progress.events_sent.clone(), progress.remaining)
    }
}

/// Objects implementing this trait have metadata that describes the request.
pub trait MetaDescriptive {
    /// Optional per-event accounting, independent of retry and finalizer decisions.
    fn delivery(&self) -> Option<RequestDelivery> {
        None
    }

    /// Returns the `RequestMetadata` associated with this object.
    fn get_metadata(&self) -> &RequestMetadata;

    // Returns a mutable reference to the `RequestMetadata` associated with this object.
    fn metadata_mut(&mut self) -> &mut RequestMetadata;
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{config::ComponentKey, internal_event::OptionalTag};

    struct DummyEvent {
        source: OptionalTag<Arc<ComponentKey>>,
        service: OptionalTag<String>,
    }

    impl GetEventCountTags for DummyEvent {
        fn get_tags(&self) -> TaggedEventsSent {
            TaggedEventsSent {
                source: self.source.clone(),
                service: self.service.clone(),
            }
        }
    }

    #[test]
    fn delivery_progress_counts_each_event_once_across_attempts() {
        let delivery = RequestDelivery::new(3);
        let retry = delivery.clone();
        delivery.record(0, CountByteSize(1, JsonSize::new(10)).into());
        retry.record(0, CountByteSize(1, JsonSize::new(10)).into());
        retry.record(2, CountByteSize(1, JsonSize::new(30)).into());
        let (sent, remaining) = delivery.snapshot();
        assert_eq!(remaining, 1);
        assert_eq!(
            sent.unwrap().size(),
            Some(CountByteSize(2, JsonSize::new(40)))
        );
    }

    #[test]
    fn delivery_progress_preserves_event_tags() {
        let delivery = RequestDelivery::new(2);
        let mut expected = HashMap::new();
        for (index, service) in ["first", "second"].into_iter().enumerate() {
            let event = DummyEvent {
                source: None.into(),
                service: Some(service.to_owned()).into(),
            };
            let mut size = GroupedCountByteSize::new_tagged();
            size.add_event(&event, JsonSize::new(12));
            expected.insert(event.get_tags(), CountByteSize(1, JsonSize::new(12)));
            delivery.record(index, size);
        }
        let (sent, remaining) = delivery.snapshot();
        assert_eq!(remaining, 0);
        assert_eq!(sent.unwrap().sizes(), Some(&expected));
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
                    TaggedEventsSent {
                        source: OptionalTag::Ignored,
                        service: Some("cabbage".to_string()).into()
                    },
                    CountByteSize(2, JsonSize::new(78))
                ),
                (
                    TaggedEventsSent {
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
