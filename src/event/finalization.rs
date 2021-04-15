#![deny(missing_docs)]

use atomig::{Atom, AtomInteger, Atomic, Ordering};
use dashmap::DashMap;
use getset::CopyGetters;
use serde::{
    de::{SeqAccess, Visitor},
    ser::SerializeSeq,
    Deserialize, Deserializer, Serialize, Serializer,
};
use std::{fmt, sync::Arc};
use tokio::sync::oneshot;
use uuid::Uuid;

type ImmutVec<T> = Box<[T]>;

lazy_static::lazy_static! {
    static ref BATCHES: DashMap<Uuid, Arc<BatchNotifier>> = DashMap::new();
    static ref EVENTS: DashMap<Uuid,Arc<EventFinalizer>> = DashMap::new();
}

/// Wrapper type for an array of event finalizers, used to support
/// custom serialization and deserialization protocols for the included
/// `Arc` elements.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EventFinalizers(ImmutVec<Arc<EventFinalizer>>);

impl EventFinalizers {
    /// Create a new array of event finalizer with the single event.
    pub fn new(finalizer: EventFinalizer) -> Self {
        Self(vec![Arc::new(finalizer)].into())
    }

    /// Merge the given list of finalizers into this array.
    pub fn merge(&mut self, other: Self) {
        if !other.0.is_empty() {
            // This requires a bit of extra work both to avoid cloning
            // the actual elements and because `self.0` cannot be
            // mutated in place.
            let finalizers = std::mem::replace(&mut self.0, vec![].into());
            let mut result: Vec<_> = finalizers.into();
            // This is the only step that may cause a (re)allocation.
            result.reserve_exact(other.0.len());
            // Box<[T]> is missing IntoIterator
            let other: Vec<_> = other.0.into();
            for entry in other.into_iter() {
                result.push(entry);
            }
            self.0 = result.into();
        }
    }
}

impl Serialize for EventFinalizers {
    /// Custom serializer for an array of event finaliers.  This
    /// registers and then serializes each finalizer as just the
    /// identifier.
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut seq = serializer.serialize_seq(Some(self.0.len()))?;
        for event in self.0.iter() {
            // Only register finalizers on serialization, to avoid the
            // expense of registration if the event is never serialized.
            EventFinalizer::register(&event);
            seq.serialize_element(&event.identifier)?;
        }
        seq.end()
    }
}

impl<'de> Deserialize<'de> for EventFinalizers {
    /// Custom serializer for an array of event finalizers. This
    /// deserializes the identifier and then looks up the associated
    /// finalizer. If the finalizer is no longer present (ie due to
    /// reload or restart), the element is skipped.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct MyVisitor;

        impl<'de> Visitor<'de> for MyVisitor {
            type Value = EventFinalizers;
            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a sequence of batch finalizer UUIDs")
            }

            fn visit_seq<S>(self, mut seq: S) -> Result<Self::Value, S::Error>
            where
                S: SeqAccess<'de>,
            {
                let mut result = Vec::new();
                while let Some(identifier) = seq.next_element::<Uuid>()? {
                    if let Some(notifier) = EventFinalizer::lookup(identifier) {
                        result.push(notifier);
                    }
                }
                Ok(EventFinalizers(result.into()))
            }
        }

        deserializer.deserialize_seq(MyVisitor)
    }
}

/// An event finalizer is the shared data required to handle tracking
/// the status of an event, and updating the status of a batch with that
/// when the event is dropped.
#[derive(CopyGetters, Debug)]
pub struct EventFinalizer {
    status: Atomic<EventStatus>,
    sources: BatchNotifiers,
    /// The unique identifier for this event
    #[get_copy = "pub"]
    identifier: Uuid,
}

impl EventFinalizer {
    /// Register a finalizer for later retrieval after serialization
    pub fn register(finalizer: &Arc<Self>) {
        // This explicitly does not overwrite existing entries, as that
        // will simply just increment and decrement the reference counts
        // because the identifier key is meant to be globally unique.
        EVENTS
            .entry(finalizer.identifier())
            .or_insert(Arc::clone(finalizer));
    }

    /// Look up a registered finalizer. TODO Some solution will be
    /// needed to clean out this table to allow drops to happen.
    pub fn lookup(identifier: Uuid) -> Option<Arc<Self>> {
        EVENTS
            .get(&identifier)
            .map(|notifier| Arc::clone(notifier.value()))
    }

    /// Create a new event in a batch. *NOTE* the sequence number MUST
    /// be unique for each event in a batch, or else this will create
    /// duplicated finalizers.
    pub fn new(batch: Arc<BatchNotifier>, sequence: u64) -> Self {
        let batch_id = batch.identifier();
        Self {
            status: Atomic::new(EventStatus::Dropped),
            sources: BatchNotifiers(vec![batch].into()),
            identifier: Uuid::new_v5(&batch_id, &sequence.to_ne_bytes()),
        }
    }

    /// Update this finalizer's status in place with the given `EventStatus`
    pub fn update_status(&self, status: EventStatus) {
        self.status
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old_status| {
                Some(old_status.update(status))
            })
            .unwrap_or_else(|_| unreachable!());
    }

    /// Update all the sources for this event with this finalizer's
    /// status, and mark this event as no longer requiring update.
    pub fn update_sources(&self) {
        let status = self
            .status
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |_| {
                Some(EventStatus::NoOp)
            })
            .unwrap_or_else(|_| unreachable!());
        self.sources.update_status(status);
    }
}

impl Drop for EventFinalizer {
    fn drop(&mut self) {
        self.update_sources();
    }
}

impl Eq for EventFinalizer {}

impl PartialEq for EventFinalizer {
    fn eq(&self, other: &Self) -> bool {
        // Only need to compare for equal identifiers because they are
        // globally unique.
        self.identifier == other.identifier
    }
}

/// Wrapper type for an array of batch notifiers, used to support custom
/// serialization and deserialization protocols for the included `Arc`
/// elements.
#[derive(Debug, Eq, PartialEq)]
pub struct BatchNotifiers(ImmutVec<Arc<BatchNotifier>>);

impl BatchNotifiers {
    fn update_status(&self, status: EventStatus) {
        if status != EventStatus::NoOp {
            for notifier in self.0.iter() {
                notifier.update_status(status);
            }
        }
    }
}

impl Serialize for BatchNotifiers {
    /// Custom serializer for the array of batch notifiers. This
    /// registers and then serializes each finalizer as just the
    /// identifier.
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut seq = serializer.serialize_seq(Some(self.0.len()))?;
        for source in self.0.iter() {
            // Only register notifiers on serialization, to avoid the
            // expense of registration if the event is never serialized.
            BatchNotifier::register(&source);
            seq.serialize_element(&source.identifier)?;
        }
        seq.end()
    }
}

impl<'de> Deserialize<'de> for BatchNotifiers {
    /// Custom serializer for the array of batch notifiers.  This
    /// deserializes the identifier and then looks up the associated
    /// notifier. If the notifier is no longer present (ie due to reload
    /// or restart), the element is skipped.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct MyVisitor;

        impl<'de> Visitor<'de> for MyVisitor {
            type Value = BatchNotifiers;
            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a sequence of batch notifier UUIDs")
            }

            fn visit_seq<S>(self, mut seq: S) -> Result<Self::Value, S::Error>
            where
                S: SeqAccess<'de>,
            {
                let mut result = Vec::new();
                while let Some(identifier) = seq.next_element::<Uuid>()? {
                    if let Some(notifier) = BatchNotifier::lookup(identifier) {
                        result.push(notifier);
                    }
                }
                Ok(BatchNotifiers(result.into()))
            }
        }

        deserializer.deserialize_seq(MyVisitor)
    }
}

/// A batch notifier contains the status
#[derive(CopyGetters, Debug)]
pub struct BatchNotifier {
    status: Atomic<BatchStatus>,
    notifier: oneshot::Sender<BatchStatus>,
    /// The unique identifier for this batch
    #[get_copy = "pub"]
    identifier: Uuid,
}

impl BatchNotifier {
    fn register(notifier: &Arc<Self>) {
        // This explicitly does not overwrite existing entries, as that
        // will simply just increment and decrement the reference counts
        // because the identifier key is meant to be globally unique.
        BATCHES
            .entry(notifier.identifier())
            .or_insert(Arc::clone(notifier));
    }

    fn lookup(identifier: Uuid) -> Option<Arc<Self>> {
        BATCHES
            .get(&identifier)
            .map(|notifier| Arc::clone(notifier.value()))
    }

    /// Create a new `BatchNotifier` along with the receiver used to
    /// await its finalization status. This takes the source identifier
    /// and a batch sequence number as parameters. *NOTE* the sequence
    /// number MUST be unique for a given source over the lifetime of
    /// the program, or else this will create duplicated notifiers.
    pub fn new_with_receiver(
        source: Uuid,
        sequence: u64,
    ) -> (Self, oneshot::Receiver<BatchStatus>) {
        let (sender, receiver) = oneshot::channel();
        let notifier = Self {
            status: Atomic::new(BatchStatus::Delivered),
            notifier: sender,
            identifier: Uuid::new_v5(&source, &sequence.to_ne_bytes()),
        };
        (notifier, receiver)
    }

    /// Update this notifier's status from the status of a finalized event.
    pub fn update_status(&self, status: EventStatus) {
        self.status
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old_status| {
                Some(old_status.update_from_event(status))
            })
            .unwrap_or_else(|_| unreachable!());
    }
}

impl Drop for BatchNotifier {
    fn drop(&mut self) {
        todo!();
    }
}

impl Eq for BatchNotifier {}

impl PartialEq for BatchNotifier {
    fn eq(&self, other: &Self) -> bool {
        // Only need to compare for equal identifiers because they are
        // globally unique.
        self.identifier == other.identifier
    }
}

/// The status of an individual batch as a whole.
#[derive(Atom, Copy, Clone, Debug, Derivative, Deserialize, Eq, PartialEq, Serialize)]
#[derivative(Default)]
#[repr(u8)]
pub enum BatchStatus {
    /// All events in the batch was accepted (the default)
    #[derivative(Default)]
    Delivered,
    /// At least one event in the batch failed delivery.
    Failed,
}

// Can be dropped when this issue is closed:
// https://github.com/LukasKalbertodt/atomig/issues/3
impl AtomInteger for BatchStatus {}

impl BatchStatus {
    /// Update this status with the new status and return the result.
    pub fn update(self, status: Self) -> Self {
        match (self, status) {
            // Delivered updates to Failed
            (Self::Delivered, _) => status,
            (_, Self::Failed) => status,
            // Otherwise, stay the same
            _ => self,
        }
    }

    /// Update this status with an `EventStatus` and return the result.
    pub fn update_from_event(self, status: EventStatus) -> Self {
        match (self, status) {
            // Delivered updates to Failed
            (Self::Delivered, EventStatus::Failed) => Self::Failed,
            // Otherwise, no change needed
            _ => self,
        }
    }
}

/// The status of an individual event.
#[derive(Atom, Copy, Clone, Debug, Derivative, Deserialize, Eq, PartialEq, Serialize)]
#[derivative(Default)]
#[repr(u8)]
pub enum EventStatus {
    /// All copies of this event were dropped without being finalized (the default).
    #[derivative(Default)]
    Dropped,
    /// All copies of this event were delivered successfully.
    Delivered,
    /// At least one copy of this event failed to be delivered.
    Failed,
    /// This status has been recorded and should not be updated.
    NoOp,
}

// Can be dropped when this issue is closed:
// https://github.com/LukasKalbertodt/atomig/issues/3
impl AtomInteger for EventStatus {}

impl EventStatus {
    /// Update this status with another event's finalization status and return the result.
    pub fn update(self, status: Self) -> Self {
        match (self, status) {
            // NoOp always overwrites existing status
            (_, Self::NoOp) => status,
            // Dropped always updates to the new status
            (Self::Dropped, _) => status,
            // NoOp is never updated
            (Self::NoOp, _) => self,
            // Delivered may update to Failed, but not to Dropped
            (Self::Delivered, Self::Dropped) => self,
            (Self::Delivered, _) => status,
            // Failed does not otherwise update
            (Self::Failed, _) => self,
        }
    }
}
