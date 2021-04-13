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

lazy_static::lazy_static! {
    static ref BATCHES: DashMap<Uuid, Arc<BatchNotifier>> = DashMap::new();
    static ref EVENTS: DashMap<Uuid,Arc<EventFinalizer>> = DashMap::new();
}

/// An event finalizer is the shared data required to handle tracking
/// the status of an event, and updating the status of a batch with that
/// when the event is dropped.
#[derive(CopyGetters, Debug, Deserialize, Serialize)]
pub struct EventFinalizer {
    status: Atomic<EventStatus>,
    #[serde(
        serialize_with = "serialize_batch_notifiers",
        deserialize_with = "deserialize_batch_notifiers"
    )]
    sources: Vec<Arc<BatchNotifier>>,
    /// The unique identifier for this event
    #[get_copy = "pub"]
    identifier: Uuid,
}

impl EventFinalizer {
    /// Register a finalizer for later retrieval after serialization
    pub fn register(finalizer: Arc<Self>) {
        // This explicitly does not overwrite existing entries, as that
        // will simply just increment and decrement the reference counts
        // because the identifier key is meant to be globally unique.
        EVENTS.entry(finalizer.identifier()).or_insert(finalizer);
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
            sources: vec![batch],
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
        if status != EventStatus::NoOp {
            for source in &self.sources {
                source.update_status(status);
            }
        }
    }
}

impl Drop for EventFinalizer {
    fn drop(&mut self) {
        self.update_sources();
    }
}

impl PartialEq for EventFinalizer {
    fn eq(&self, other: &Self) -> bool {
        (self.sources.iter())
            .zip(other.sources.iter())
            .all(|(a, b)| a.identifier == b.identifier)
            && self.status.load(Ordering::Relaxed) == other.status.load(Ordering::Relaxed)
    }
}

impl Eq for EventFinalizer {}

/// Custom serializer for the array of batch notifiers. This form is
/// required because we can't implmenet a serializer for `Arc<T>`.  This
/// registers and then serializes each finalizer as just the identifier.
fn serialize_batch_notifiers<S: Serializer>(
    sources: &[Arc<BatchNotifier>],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let mut seq = serializer.serialize_seq(Some(sources.len()))?;
    for source in sources {
        // Only register notifiers on serialization, to avoid the
        // expense of registration if the event is never serialized.
        BatchNotifier::register(Arc::clone(&source));
        seq.serialize_element(&source.identifier)?;
    }
    seq.end()
}

/// Custom serializer for the array of batch notifiers. This form is
/// required because we can't implmenet a deserializer for `Arc<T>`.
/// This deserializes the identifier and then looks up the associated
/// notifier. If the notifier is no longer present (ie due to reload or
/// restart), the element is skipped.
fn deserialize_batch_notifiers<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<Arc<BatchNotifier>>, D::Error> {
    struct NotifierVisitor;

    impl<'de> Visitor<'de> for NotifierVisitor {
        type Value = Vec<Arc<BatchNotifier>>;
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
            Ok(result)
        }
    }

    deserializer.deserialize_seq(NotifierVisitor)
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
    fn register(notifier: Arc<Self>) {
        // This explicitly does not overwrite existing entries, as that
        // will simply just increment and decrement the reference counts
        // because the identifier key is meant to be globally unique.
        BATCHES.entry(notifier.identifier()).or_insert(notifier);
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
