#![deny(missing_docs)]

use std::{cmp, future::Future, mem, pin::Pin, sync::Arc, task::Poll};

use atomig::{Atom, Atomic, Ordering};
use futures::future::FutureExt;
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

use super::Event;
use crate::ByteSizeOf;

/// Wrapper type for an array of event finalizers. This is the primary
/// public interface to event finalization metadata.
#[derive(Clone, Debug, Default)]
pub struct EventFinalizers(Vec<Arc<EventFinalizer>>);

impl Eq for EventFinalizers {}

impl PartialEq for EventFinalizers {
    fn eq(&self, other: &Self) -> bool {
        self.0.len() == other.0.len()
            && (self.0.iter())
                .zip(other.0.iter())
                .all(|(a, b)| Arc::ptr_eq(a, b))
    }
}

impl PartialOrd for EventFinalizers {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        // There is no partial order defined structurally on
        // `EventFinalizer`. Partial equality is defined on the equality of
        // `Arc`s. Therefore, partial ordering of `EventFinalizers` is defined
        // only on the length of the finalizers.
        self.0.len().partial_cmp(&other.0.len())
    }
}

impl ByteSizeOf for EventFinalizers {
    fn allocated_bytes(&self) -> usize {
        self.0.iter().fold(0, |acc, arc| acc + arc.size_of())
    }
}

impl EventFinalizers {
    /// Create a new array of event finalizer with the single event.
    pub fn new(finalizer: EventFinalizer) -> Self {
        Self(vec![Arc::new(finalizer)])
    }

    /// Add a single finalizer to this array.
    pub fn add(&mut self, finalizer: EventFinalizer) {
        self.0.push(Arc::new(finalizer));
    }

    /// Merge the given list of finalizers into this array.
    pub fn merge(&mut self, other: Self) {
        self.0.extend(other.0.into_iter());
    }

    /// Update the status of all finalizers in this set.
    pub fn update_status(&self, status: EventStatus) {
        for finalizer in &self.0 {
            finalizer.update_status(status);
        }
    }

    /// Update all sources for this finalizer with the current
    /// status. This *drops* the finalizer array elements so they may
    /// immediately signal the source batch.
    pub fn update_sources(&mut self) {
        let finalizers = mem::take(&mut self.0);
        for finalizer in &finalizers {
            finalizer.update_batch();
        }
    }

    #[cfg(test)]
    fn count_finalizers(&self) -> usize {
        self.0.len()
    }
}

impl Finalizable for EventFinalizers {
    fn take_finalizers(&mut self) -> EventFinalizers {
        mem::take(self)
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

impl ByteSizeOf for EventFinalizer {
    fn allocated_bytes(&self) -> usize {
        0
    }
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

/// A convenience newtype wrapper for the one-shot receiver for an
/// individual batch status.
#[pin_project::pin_project]
pub struct BatchStatusReceiver(oneshot::Receiver<BatchStatus>);

impl Future for BatchStatusReceiver {
    type Output = BatchStatus;
    fn poll(mut self: Pin<&mut Self>, ctx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        match self.0.poll_unpin(ctx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Ok(status)) => Poll::Ready(status),
            Poll::Ready(Err(error)) => {
                error!(message = "Batch status receiver dropped before sending.", %error);
                Poll::Ready(BatchStatus::Errored)
            }
        }
    }
}

impl BatchStatusReceiver {
    /// Wrapper for the underlying `try_recv` function.
    ///
    /// # Errors
    ///
    /// - `TryRecvError::Empty` if no value has been sent yet.
    /// - `TryRecvError::Closed` if the sender has dropped without sending a value.
    pub fn try_recv(&mut self) -> Result<BatchStatus, oneshot::error::TryRecvError> {
        self.0.try_recv()
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
    pub fn new_with_receiver() -> (Arc<Self>, BatchStatusReceiver) {
        let (sender, receiver) = oneshot::channel();
        let notifier = Self {
            status: Atomic::new(BatchStatus::Delivered),
            notifier: Some(sender),
        };
        (Arc::new(notifier), BatchStatusReceiver(receiver))
    }

    /// Optionally call `new_with_receiver` and wrap the result in `Option`s
    pub fn maybe_new_with_receiver(
        enabled: bool,
    ) -> (Option<Arc<Self>>, Option<BatchStatusReceiver>) {
        if enabled {
            let (batch, receiver) = Self::new_with_receiver();
            (Some(batch), Some(receiver))
        } else {
            (None, None)
        }
    }

    /// Apply a new batch notifier to a batch of events, and return the receiver.
    pub fn maybe_apply_to_events(
        enabled: bool,
        events: &mut [Event],
    ) -> Option<BatchStatusReceiver> {
        enabled.then(|| {
            let (batch, receiver) = Self::new_with_receiver();
            for event in events {
                event.add_batch_notifier(Arc::clone(&batch));
            }
            receiver
        })
    }

    /// Update this notifier's status from the status of a finalized event.
    #[allow(clippy::missing_panics_doc)] // Panic is unreachable
    fn update_status(&self, status: EventStatus) {
        // The status starts as Delivered and can only change if the new
        // status is different than that.
        if status != EventStatus::Delivered && status != EventStatus::Dropped {
            self.status
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old_status| {
                    Some(old_status.update(status))
                })
                .unwrap_or_else(|_| unreachable!());
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
    /// At least one event in the batch had a transient error in delivery.
    Errored,
    /// At least one event in the batch had a permanent failure or rejection.
    Rejected,
}

impl BatchStatus {
    /// Update this status with another batch's delivery status, and return the
    /// result.
    #[allow(clippy::match_same_arms)] // False positive: https://github.com/rust-lang/rust-clippy/issues/860
    fn update(self, status: EventStatus) -> Self {
        match (self, status) {
            // `Dropped` and `Delivered` do not change the status.
            (_, EventStatus::Dropped) | (_, EventStatus::Delivered) => self,
            // `Rejected` overrides `Errored` and `Delivered`
            (Self::Rejected, _) | (_, EventStatus::Rejected) => Self::Rejected,
            // `Errored` overrides `Delivered`
            (Self::Errored, _) | (_, EventStatus::Errored) => Self::Errored,
            // No change for `Delivered`
            _ => self,
        }
    }
}

/// The status of an individual event.
#[derive(Atom, Copy, Clone, Debug, Derivative, Deserialize, Eq, PartialEq, Serialize)]
#[derivative(Default)]
#[repr(u8)]
pub enum EventStatus {
    /// All copies of this event were dropped without being finalized (the
    /// default).
    #[derivative(Default)]
    Dropped,
    /// All copies of this event were delivered successfully.
    Delivered,
    /// At least one copy of this event encountered a retriable error.
    Errored,
    /// At least one copy of this event encountered a permanent failure or
    /// rejection.
    Rejected,
    /// This status has been recorded and should not be updated.
    Recorded,
}

impl EventStatus {
    /// Update this status with another event's finalization status and return
    /// the result.
    ///
    /// # Panics
    ///
    /// Passing a new status of `Dropped` is a programming error and
    /// will panic in debug/test builds.
    #[allow(clippy::match_same_arms)] // False positive: https://github.com/rust-lang/rust-clippy/issues/860
    pub fn update(self, status: Self) -> Self {
        match (self, status) {
            // `Recorded` always overwrites existing status and is never updated
            (_, Self::Recorded) | (Self::Recorded, _) => Self::Recorded,
            // `Dropped` always updates to the new status.
            (Self::Dropped, _) => status,
            // Updates *to* `Dropped` are nonsense.
            (_, Self::Dropped) => {
                debug_assert!(false, "Updating EventStatus to Dropped is nonsense");
                self
            }
            // `Rejected` overrides `Errored` or `Delivered`.
            (Self::Rejected, _) | (_, Self::Rejected) => Self::Rejected,
            // `Errored` overrides `Delivered`.
            (Self::Errored, _) | (_, Self::Errored) => Self::Errored,
            // No change for `Delivered`.
            (Self::Delivered, Self::Delivered) => Self::Delivered,
        }
    }
}

/// An object that can be finalized.
pub trait Finalizable {
    /// Consumes the finalizers of this object.
    ///
    /// Typically used for coalescing the finalizers of multiple items, such as
    /// when batching finalizable objects where all finalizations will be
    /// processed when the batch itself is processed.
    fn take_finalizers(&mut self) -> EventFinalizers;
}

impl<T: Finalizable> Finalizable for Vec<T> {
    fn take_finalizers(&mut self) -> EventFinalizers {
        self.iter_mut()
            .fold(EventFinalizers::default(), |mut acc, x| {
                acc.merge(x.take_finalizers());
                acc
            })
    }
}

#[cfg(test)]
mod tests {
    use tokio::sync::oneshot::error::TryRecvError::Empty;

    use super::*;

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
        fin.update_status(EventStatus::Rejected);
        assert_eq!(receiver.try_recv(), Err(Empty));
        fin.update_sources();
        assert_eq!(fin.count_finalizers(), 0);
        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Rejected));
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

    #[ignore] // The current implementation does not deduplicate finalizers
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

    fn make_finalizer() -> (EventFinalizers, BatchStatusReceiver) {
        let (batch, receiver) = BatchNotifier::new_with_receiver();
        let finalizer = EventFinalizers::new(EventFinalizer::new(batch));
        assert_eq!(finalizer.count_finalizers(), 1);
        (finalizer, receiver)
    }

    #[test]
    fn event_status_updates() {
        use EventStatus::{Delivered, Dropped, Errored, Recorded, Rejected};

        assert_eq!(Dropped.update(Dropped), Dropped);
        assert_eq!(Dropped.update(Delivered), Delivered);
        assert_eq!(Dropped.update(Errored), Errored);
        assert_eq!(Dropped.update(Rejected), Rejected);
        assert_eq!(Dropped.update(Recorded), Recorded);

        //assert_eq!(Delivered.update(Dropped), Delivered);
        assert_eq!(Delivered.update(Delivered), Delivered);
        assert_eq!(Delivered.update(Errored), Errored);
        assert_eq!(Delivered.update(Rejected), Rejected);
        assert_eq!(Delivered.update(Recorded), Recorded);

        //assert_eq!(Errored.update(Dropped), Errored);
        assert_eq!(Errored.update(Delivered), Errored);
        assert_eq!(Errored.update(Errored), Errored);
        assert_eq!(Errored.update(Rejected), Rejected);
        assert_eq!(Errored.update(Recorded), Recorded);

        //assert_eq!(Rejected.update(Dropped), Rejected);
        assert_eq!(Rejected.update(Delivered), Rejected);
        assert_eq!(Rejected.update(Errored), Rejected);
        assert_eq!(Rejected.update(Rejected), Rejected);
        assert_eq!(Rejected.update(Recorded), Recorded);

        //assert_eq!(Recorded.update(Dropped), Recorded);
        assert_eq!(Recorded.update(Delivered), Recorded);
        assert_eq!(Recorded.update(Errored), Recorded);
        assert_eq!(Recorded.update(Rejected), Recorded);
        assert_eq!(Recorded.update(Recorded), Recorded);
    }

    #[test]
    fn batch_status_update() {
        use BatchStatus::{Delivered, Errored, Rejected};

        assert_eq!(Delivered.update(EventStatus::Dropped), Delivered);
        assert_eq!(Delivered.update(EventStatus::Delivered), Delivered);
        assert_eq!(Delivered.update(EventStatus::Errored), Errored);
        assert_eq!(Delivered.update(EventStatus::Rejected), Rejected);
        assert_eq!(Delivered.update(EventStatus::Recorded), Delivered);

        assert_eq!(Errored.update(EventStatus::Dropped), Errored);
        assert_eq!(Errored.update(EventStatus::Delivered), Errored);
        assert_eq!(Errored.update(EventStatus::Errored), Errored);
        assert_eq!(Errored.update(EventStatus::Rejected), Rejected);
        assert_eq!(Errored.update(EventStatus::Recorded), Errored);

        assert_eq!(Rejected.update(EventStatus::Dropped), Rejected);
        assert_eq!(Rejected.update(EventStatus::Delivered), Rejected);
        assert_eq!(Rejected.update(EventStatus::Errored), Rejected);
        assert_eq!(Rejected.update(EventStatus::Rejected), Rejected);
        assert_eq!(Rejected.update(EventStatus::Recorded), Rejected);
    }
}
