//! Per-shard in-flight sequence tracker for at-least-once delivery.
//!
//! Tracks how many records are currently in-flight (sent but not yet acknowledged)
//! and advances the acked sequence number only when all prior records are confirmed.

use std::sync::{
    Arc,
    atomic::{AtomicI64, Ordering},
    Mutex,
};

/// Tracks in-flight records and the last fully-acknowledged sequence number.
///
/// This implements the same semantics as `checkpoint.NewCapped` from the Go source:
/// the acked sequence is only advanced once all batches at or before that point
/// have been acknowledged by downstream sinks.
#[derive(Clone)]
pub struct SequenceTracker {
    /// Configured cap on in-flight records.
    limit: i64,
    /// How many records are currently sent but not acknowledged.
    in_flight: Arc<AtomicI64>,
    /// The last fully-acknowledged sequence number.
    acked_sequence: Arc<Mutex<String>>,
}

impl SequenceTracker {
    pub fn new(limit: u32, initial_sequence: String) -> Self {
        Self {
            limit: limit as i64,
            in_flight: Arc::new(AtomicI64::new(0)),
            acked_sequence: Arc::new(Mutex::new(initial_sequence)),
        }
    }

    /// Returns true when more records can be accepted without exceeding the cap.
    pub fn can_accept(&self, count: i64) -> bool {
        self.in_flight.load(Ordering::Acquire) + count <= self.limit
    }

    /// Account for `count` new records being sent in-flight.
    pub fn track(&self, count: i64) {
        self.in_flight.fetch_add(count, Ordering::AcqRel);
    }

    /// Acknowledge `count` records and update the acked sequence to `sequence`
    /// if it is lexicographically greater than the current value.
    pub fn acknowledge(&self, count: i64, sequence: String) {
        self.in_flight.fetch_sub(count, Ordering::AcqRel);
        let mut acked = self.acked_sequence.lock().expect("tracker lock poisoned");
        if sequence > *acked {
            *acked = sequence;
        }
    }

    /// Release `count` records without advancing the sequence (used on errors).
    pub fn release(&self, count: i64) {
        self.in_flight.fetch_sub(count, Ordering::AcqRel);
    }

    /// Advance the acked sequence without modifying the in-flight counter.
    ///
    /// Use this when records were fetched from Kinesis but produced no events
    /// after decoding, so `track` was never called for them. Calling `acknowledge`
    /// in that case would incorrectly decrement `in_flight` below zero.
    pub fn advance_sequence(&self, sequence: String) {
        let mut acked = self.acked_sequence.lock().expect("tracker lock poisoned");
        if sequence > *acked {
            *acked = sequence;
        }
    }

    /// Returns the most recently fully-acknowledged sequence number.
    pub fn acked_sequence(&self) -> String {
        self.acked_sequence
            .lock()
            .expect("tracker lock poisoned")
            .clone()
    }
}
