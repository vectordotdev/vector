//! Per-shard in-flight sequence tracker for at-least-once delivery.
//!
//! Tracks how many records are currently in-flight (sent but not yet acknowledged)
//! and advances the acked sequence number only through a contiguous delivered prefix.
//! A later batch that finishes first does not move the checkpoint past an earlier gap.

use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

/// Tracks in-flight records and the last fully-acknowledged sequence number.
///
/// Batches are recorded in the order they were read from the shard. The acked
/// sequence moves forward only while every batch from the front of that queue
/// has been delivered. A rejection leaves a gap so a restart replays from the
/// last contiguous sequence.
#[derive(Clone)]
pub struct SequenceTracker {
    /// Configured cap on in-flight records. At least 1.
    limit: i64,
    state: Arc<Mutex<TrackerState>>,
    settled: Arc<Notify>,
}

struct TrackerState {
    batches: VecDeque<InFlightBatch>,
    acked_sequence: String,
    /// True once any batch was rejected. Stays set so a finished shard is not
    /// checkpointed as `SHARD_END` after a gap.
    rejected: bool,
    in_flight: i64,
}

struct InFlightBatch {
    sequence: String,
    count: i64,
    status: BatchStatus,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BatchStatus {
    Pending,
    Delivered,
    Rejected,
}

impl SequenceTracker {
    pub fn new(limit: u32, initial_sequence: String) -> Self {
        let limit = i64::from(limit.max(1));
        Self {
            limit,
            state: Arc::new(Mutex::new(TrackerState {
                batches: VecDeque::new(),
                acked_sequence: initial_sequence,
                rejected: false,
                in_flight: 0,
            })),
            settled: Arc::new(Notify::new()),
        }
    }

    /// Returns true when `count` more records fit under the cap.
    pub fn can_accept(&self, count: i64) -> bool {
        self.remaining_capacity() >= count
    }

    /// How many additional records can be read without passing the cap.
    pub fn remaining_capacity(&self) -> i64 {
        let state = self.state.lock().expect("tracker lock poisoned");
        (self.limit - state.in_flight).max(0)
    }

    /// Account for `count` records from a batch whose last sequence is `sequence`.
    pub fn track(&self, count: i64, sequence: String) {
        if count <= 0 {
            return;
        }
        let mut state = self.state.lock().expect("tracker lock poisoned");
        state.in_flight += count;
        state.batches.push_back(InFlightBatch {
            sequence,
            count,
            status: BatchStatus::Pending,
        });
    }

    /// Mark the batch ending at `sequence` as delivered and advance through
    /// any newly completed prefix.
    pub fn acknowledge(&self, sequence: String) {
        self.settle(sequence, BatchStatus::Delivered);
    }

    /// Mark the batch ending at `sequence` as rejected. The checkpoint does not
    /// move past it.
    pub fn release(&self, sequence: String) {
        self.settle(sequence, BatchStatus::Rejected);
    }

    /// Record a sequence that produced no events as already delivered.
    ///
    /// Used when Kinesis returned records that decoded to nothing, so `track`
    /// was never called. The sequence still cannot pass an earlier pending batch.
    pub fn advance_sequence(&self, sequence: String) {
        let mut state = self.state.lock().expect("tracker lock poisoned");
        state.batches.push_back(InFlightBatch {
            sequence,
            count: 0,
            status: BatchStatus::Delivered,
        });
        state.drain_delivered_prefix();
        drop(state);
        self.settled.notify_waiters();
    }

    /// Returns the last sequence for which every earlier batch was delivered.
    pub fn acked_sequence(&self) -> String {
        self.state
            .lock()
            .expect("tracker lock poisoned")
            .acked_sequence
            .clone()
    }

    /// True when any batch was rejected. A rejected shard must not be sealed
    /// with `SHARD_END`.
    pub fn has_rejection(&self) -> bool {
        self.state.lock().expect("tracker lock poisoned").rejected
    }

    /// Records still waiting on a downstream acknowledgement.
    pub fn in_flight(&self) -> i64 {
        self.state.lock().expect("tracker lock poisoned").in_flight
    }

    /// Wait until every tracked batch has been delivered or rejected, or until
    /// `cancel` is cancelled.
    pub async fn wait_until_settled(&self, cancel: &CancellationToken) {
        loop {
            let notified = self.settled.notified();
            tokio::pin!(notified);
            if self.in_flight() == 0 {
                return;
            }
            tokio::select! {
                _ = notified => {}
                _ = cancel.cancelled() => return,
            }
        }
    }

    fn settle(&self, sequence: String, status: BatchStatus) {
        let mut state = self.state.lock().expect("tracker lock poisoned");
        if let Some(index) = state
            .batches
            .iter()
            .position(|batch| batch.status == BatchStatus::Pending && batch.sequence == sequence)
        {
            let count = state.batches[index].count;
            state.in_flight -= count;
            state.batches[index].status = status;
            if status == BatchStatus::Rejected {
                state.rejected = true;
            }
        }
        state.drain_delivered_prefix();
        drop(state);
        self.settled.notify_waiters();
    }
}

impl TrackerState {
    fn drain_delivered_prefix(&mut self) {
        while self
            .batches
            .front()
            .is_some_and(|batch| batch.status == BatchStatus::Delivered)
        {
            if let Some(batch) = self.batches.pop_front() {
                self.acked_sequence = batch.sequence;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn out_of_order_ack_does_not_pass_a_gap() {
        let tracker = SequenceTracker::new(10, String::new());
        tracker.track(1, "1".to_string());
        tracker.track(1, "2".to_string());

        tracker.acknowledge("2".to_string());
        assert_eq!(tracker.acked_sequence(), "");
        assert_eq!(tracker.in_flight(), 1);

        tracker.acknowledge("1".to_string());
        assert_eq!(tracker.acked_sequence(), "2");
        assert_eq!(tracker.in_flight(), 0);
    }

    #[test]
    fn rejection_blocks_later_delivered_sequences() {
        let tracker = SequenceTracker::new(10, "0".to_string());
        tracker.track(2, "1".to_string());
        tracker.track(2, "2".to_string());

        tracker.acknowledge("2".to_string());
        tracker.release("1".to_string());

        assert_eq!(tracker.acked_sequence(), "0");
        assert!(tracker.has_rejection());
        assert_eq!(tracker.in_flight(), 0);
        assert_eq!(tracker.remaining_capacity(), 10);
    }

    #[test]
    fn advance_sequence_waits_for_earlier_inflight_batches() {
        let tracker = SequenceTracker::new(10, String::new());
        tracker.track(1, "1".to_string());
        tracker.advance_sequence("2".to_string());
        assert_eq!(tracker.acked_sequence(), "");

        tracker.acknowledge("1".to_string());
        assert_eq!(tracker.acked_sequence(), "2");
    }

    #[test]
    fn remaining_capacity_shrinks_as_records_are_tracked() {
        let tracker = SequenceTracker::new(1, String::new());
        assert_eq!(tracker.remaining_capacity(), 1);
        assert!(tracker.can_accept(1));
        assert!(!tracker.can_accept(2));

        tracker.track(1, "1".to_string());
        assert_eq!(tracker.remaining_capacity(), 0);
        assert!(!tracker.can_accept(1));

        tracker.acknowledge("1".to_string());
        assert_eq!(tracker.remaining_capacity(), 1);
    }

    #[test]
    fn zero_limit_still_accepts_one_record() {
        let tracker = SequenceTracker::new(0, String::new());
        assert_eq!(tracker.remaining_capacity(), 1);
    }
}
