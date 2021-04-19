#![deny(missing_docs)]

use atomig::{Atom, AtomInteger, Atomic, Ordering};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::oneshot;

type ImmutVec<T> = Box<[T]>;

/// Wrapper type for an optional finalizer,
/// to go into the top-level event metadata.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MaybeEventFinalizer(Option<EventFinalizers>);

impl MaybeEventFinalizer {
    /// Merge another finalizer into this one.
    pub fn merge(&mut self, other: Self) {
        self.0 = match (self.0.take(), other.0) {
            (None, None) => None,
            (Some(f), None) => Some(f),
            (None, Some(f)) => Some(f),
            (Some(mut f1), Some(f2)) => {
                f1.merge(f2);
                Some(f1)
            }
        };
    }
}

impl From<EventFinalizer> for MaybeEventFinalizer {
    fn from(finalizer: EventFinalizer) -> Self {
        Self(Some(EventFinalizers::new(finalizer)))
    }
}

/// Wrapper type for an array of event finalizers.
#[derive(Clone, Debug)]
struct EventFinalizers(ImmutVec<Arc<EventFinalizer>>);

impl Eq for EventFinalizers {}

impl PartialEq for EventFinalizers {
    fn eq(&self, other: &Self) -> bool {
        (self.0.iter())
            .zip(other.0.iter())
            .all(|(a, b)| Arc::ptr_eq(a, b))
    }
}

impl EventFinalizers {
    /// Create a new array of event finalizer with the single event.
    fn new(finalizer: EventFinalizer) -> Self {
        Self(vec![Arc::new(finalizer)].into())
    }

    /// Merge the given list of finalizers into this array.
    fn merge(&mut self, other: Self) {
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

/// An event finalizer is the shared data required to handle tracking
/// the status of an event, and updating the status of a batch with that
/// when the event is dropped.
#[derive(Debug)]
pub struct EventFinalizer {
    status: Atomic<EventStatus>,
    sources: BatchNotifiers,
}

impl EventFinalizer {
    /// Create a new event in a batch.
    pub fn new(batch: Arc<BatchNotifier>) -> Self {
        Self {
            status: Atomic::new(EventStatus::Dropped),
            sources: BatchNotifiers(vec![batch].into()),
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

/// Wrapper type for an array of batch notifiers.
#[derive(Debug)]
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

/// A batch notifier contains the status of the current batch along with
/// a one-shot notifier to send that status back to the source. It is
/// shared among all events of a batch.
#[derive(Debug)]
pub struct BatchNotifier {
    status: Atomic<BatchStatus>,
    notifier: Option<oneshot::Sender<BatchStatus>>,
}

impl BatchNotifier {
    /// Create a new `BatchNotifier` along with the receiver used to
    /// await its finalization status.
    pub fn new_with_receiver() -> (Self, oneshot::Receiver<BatchStatus>) {
        let (sender, receiver) = oneshot::channel();
        let notifier = Self {
            status: Atomic::new(BatchStatus::Delivered),
            notifier: Some(sender),
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

    /// Send this notifier's status up to the source
    pub fn send_status(&mut self) {
        if let Some(notifier) = self.notifier.take() {
            let status = self.status.load(Ordering::Relaxed);
            if notifier.send(status).is_err() {
                warn!(message = "Could not send batch acknowledgement notifier");
            }
        }
    }
}

impl Drop for BatchNotifier {
    fn drop(&mut self) {
        self.send_status();
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
