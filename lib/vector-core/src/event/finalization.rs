#![deny(missing_docs)]

use atomig::{Atom, AtomInteger, Atomic, Ordering};
use serde::{Deserialize, Serialize};
use std::{mem, sync::Arc};
use tokio::sync::oneshot;

type ImmutVec<T> = Box<[T]>;

/// Wrapper type for an array of event finalizers. This is the primary
/// public interface to event finalization metadata.
#[derive(Clone, Debug, Default)]
pub struct EventFinalizers(ImmutVec<Arc<EventFinalizer>>);

impl Eq for EventFinalizers {}

impl PartialEq for EventFinalizers {
    fn eq(&self, other: &Self) -> bool {
        self.0.len() == other.0.len()
            && (self.0.iter())
                .zip(other.0.iter())
                .all(|(a, b)| Arc::ptr_eq(a, b))
    }
}

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
            let finalizers = mem::replace(&mut self.0, vec![].into());
            let mut result: Vec<_> = finalizers.into();
            // This is the only step that may cause a (re)allocation.
            result.reserve_exact(other.0.len());
            // Box<[T]> is missing IntoIterator
            let other: Vec<_> = other.0.into();
            for entry in other {
                // Deduplicate by hand, assume the list is trivially small
                if !result.iter().any(|existing| Arc::ptr_eq(existing, &entry)) {
                    result.push(entry);
                }
            }
            self.0 = result.into();
        }
    }

    /// Update the status of all finalizers in this set.
    pub fn update_status(&self, status: EventStatus) {
        for finalizer in self.0.iter() {
            finalizer.update_status(status);
        }
    }

    /// Update all sources for this finalizer with the current
    /// status. This *drops* the finalizer array elements so they may
    /// immediately signal the source batch.
    pub fn update_sources(&mut self) {
        let finalizers = mem::replace(&mut self.0, vec![].into());
        for finalizer in finalizers.iter() {
            finalizer.update_batch();
        }
    }

    #[cfg(test)]
    fn count_finalizers(&self) -> usize {
        self.0.len()
    }
}

/// An event finalizer is the shared data required to handle tracking
/// the status of an event, and updating the status of a batch with that
/// when the event is dropped.
#[derive(Debug)]
pub struct EventFinalizer {
    status: Atomic<EventStatus>,
    batch: Arc<BatchNotifier>,
}

impl EventFinalizer {
    /// Create a new event in a batch.
    pub fn new(batch: Arc<BatchNotifier>) -> Self {
        let status = Atomic::new(EventStatus::Dropped);
        Self { status, batch }
    }

    /// Update this finalizer's status in place with the given `EventStatus`.
    #[allow(clippy::missing_panics_doc)] // Panic is unreachable
    pub fn update_status(&self, status: EventStatus) {
        self.status
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old_status| {
                Some(old_status.update(status))
            })
            .unwrap_or_else(|_| unreachable!());
    }

    /// Update the batch for this event with this finalizer's
    /// status, and mark this event as no longer requiring update.
    #[allow(clippy::missing_panics_doc)] // Panic is unreachable
    pub fn update_batch(&self) {
        let status = self
            .status
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |_| {
                Some(EventStatus::Recorded)
            })
            .unwrap_or_else(|_| unreachable!());
        self.batch.update_status(status);
    }
}

impl Drop for EventFinalizer {
    fn drop(&mut self) {
        self.update_batch();
    }
}

/// A convenience type alias for the one-shot receiver for an individual
/// batch status.
pub type BatchStatusReceiver = oneshot::Receiver<BatchStatus>;

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
    pub fn new_with_receiver() -> (Arc<Self>, BatchStatusReceiver) {
        let (sender, receiver) = oneshot::channel();
        let notifier = Self {
            status: Atomic::new(BatchStatus::Delivered),
            notifier: Some(sender),
        };
        (Arc::new(notifier), receiver)
    }

    /// Update this notifier's status from the status of a finalized event.
    fn update_status(&self, status: EventStatus) {
        // The status starts as Delivered and can only change to Failed
        // here. A store cycle is much faster than fetch+update.
        if status == EventStatus::Failed {
            self.status.store(BatchStatus::Failed, Ordering::Relaxed);
        }
    }

    /// Send this notifier's status up to the source.
    fn send_status(&mut self) {
        if let Some(notifier) = self.notifier.take() {
            let status = self.status.load(Ordering::Relaxed);
            // Ignore the error case, as it will happen during normal
            // source shutdown and we can't detect that here.
            let _ = notifier.send(status);
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
    /// All events in the batch were accepted (the default)
    #[derivative(Default)]
    Delivered,
    /// At least one event in the batch failed delivery.
    Failed,
}

// Can be dropped when this issue is closed:
// https://github.com/LukasKalbertodt/atomig/issues/3
impl AtomInteger for BatchStatus {}

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
    Recorded,
}

// Can be dropped when this issue is closed:
// https://github.com/LukasKalbertodt/atomig/issues/3
impl AtomInteger for EventStatus {}

impl EventStatus {
    /// Update this status with another event's finalization status and return the result.
    #[allow(clippy::match_same_arms)] // https://github.com/rust-lang/rust-clippy/issues/860
    pub fn update(self, status: Self) -> Self {
        match (self, status) {
            // Recorded always overwrites existing status.
            (_, Self::Recorded)
            // Dropped always updates to the new status.
                | (Self::Dropped, _) => status,
            // Recorded is never updated.
            (Self::Recorded, _)
            // Delivered may update to `Failed`, but not to `Dropped`.
                | (Self::Delivered, Self::Dropped)
            // Failed does not otherwise update.
                | (Self::Failed, _) => self,
            (Self::Delivered, _) => status,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::oneshot::{error::TryRecvError::Empty, Receiver};

    #[test]
    fn defaults() {
        let finalizer = EventFinalizers::default();
        assert_eq!(finalizer.count_finalizers(), 0);
    }

    #[test]
    fn sends_notification() {
        let (fin, mut receiver) = make_finalizer();
        assert_eq!(receiver.try_recv(), Err(Empty));
        drop(fin);
        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));
    }

    #[test]
    fn early_update() {
        let (mut fin, mut receiver) = make_finalizer();
        fin.update_status(EventStatus::Failed);
        assert_eq!(receiver.try_recv(), Err(Empty));
        fin.update_sources();
        assert_eq!(fin.count_finalizers(), 0);
        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Failed));
    }

    #[test]
    fn clone_events() {
        let (fin1, mut receiver) = make_finalizer();
        let fin2 = fin1.clone();
        assert_eq!(fin1.count_finalizers(), 1);
        assert_eq!(fin2.count_finalizers(), 1);
        assert_eq!(fin1, fin2);

        assert_eq!(receiver.try_recv(), Err(Empty));
        drop(fin1);
        assert_eq!(receiver.try_recv(), Err(Empty));
        drop(fin2);
        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));
    }

    #[test]
    fn merge_events() {
        let mut fin0 = EventFinalizers::default();
        let (fin1, mut receiver1) = make_finalizer();
        let (fin2, mut receiver2) = make_finalizer();

        assert_eq!(fin0.count_finalizers(), 0);
        fin0.merge(fin1);
        assert_eq!(fin0.count_finalizers(), 1);
        fin0.merge(fin2);
        assert_eq!(fin0.count_finalizers(), 2);

        assert_eq!(receiver1.try_recv(), Err(Empty));
        assert_eq!(receiver2.try_recv(), Err(Empty));
        drop(fin0);
        assert_eq!(receiver1.try_recv(), Ok(BatchStatus::Delivered));
        assert_eq!(receiver2.try_recv(), Ok(BatchStatus::Delivered));
    }

    #[test]
    fn clone_and_merge_events() {
        let (mut fin1, mut receiver) = make_finalizer();
        let fin2 = fin1.clone();
        fin1.merge(fin2);
        assert_eq!(fin1.count_finalizers(), 1);

        assert_eq!(receiver.try_recv(), Err(Empty));
        drop(fin1);
        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));
    }

    #[test]
    fn multi_event_batch() {
        let (batch, mut receiver) = BatchNotifier::new_with_receiver();
        let event1 = EventFinalizers::new(EventFinalizer::new(Arc::clone(&batch)));
        let mut event2 = EventFinalizers::new(EventFinalizer::new(Arc::clone(&batch)));
        let event3 = EventFinalizers::new(EventFinalizer::new(Arc::clone(&batch)));
        // Also clone one…
        let event4 = event1.clone();
        drop(batch);
        assert_eq!(event1.count_finalizers(), 1);
        assert_eq!(event2.count_finalizers(), 1);
        assert_eq!(event3.count_finalizers(), 1);
        assert_eq!(event4.count_finalizers(), 1);
        assert_ne!(event1, event2);
        assert_ne!(event1, event3);
        assert_eq!(event1, event4);
        assert_ne!(event2, event3);
        assert_ne!(event2, event4);
        assert_ne!(event3, event4);
        // …and merge another
        event2.merge(event3);
        assert_eq!(event2.count_finalizers(), 2);

        assert_eq!(receiver.try_recv(), Err(Empty));
        drop(event1);
        assert_eq!(receiver.try_recv(), Err(Empty));
        drop(event2);
        assert_eq!(receiver.try_recv(), Err(Empty));
        drop(event4);
        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));
    }

    fn make_finalizer() -> (EventFinalizers, Receiver<BatchStatus>) {
        let (batch, receiver) = BatchNotifier::new_with_receiver();
        let finalizer = EventFinalizers::new(EventFinalizer::new(batch));
        assert_eq!(finalizer.count_finalizers(), 1);
        (finalizer, receiver)
    }
}
