use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};

use tokio::time::interval;
use tracing::{Instrument, Span};
use vector_common::internal_event::emit;

use crate::{
    internal_events::{BufferCreated, BufferEventsDropped, BufferEventsReceived, BufferEventsSent},
    spawn_named,
};

/// Since none of the values used with atomic operations are used to protect other values, we can
/// always used a "relaxed" ordering when updating them.
const ORDERING: Ordering = Ordering::Relaxed;

fn increment_counter(counter: &AtomicU64, delta: u64) {
    counter
        .fetch_update(ORDERING, ORDERING, |current| {
            Some(current.checked_add(delta).unwrap_or_else(|| {
                warn!(
                    current,
                    delta, "Buffer counter overflowed. Clamping value to `u64::MAX`."
                );
                u64::MAX
            }))
        })
        .ok();
}

fn decrement_counter(counter: &AtomicU64, delta: u64) {
    counter
        .fetch_update(ORDERING, ORDERING, |current| {
            Some(current.checked_sub(delta).unwrap_or_else(|| {
                warn!(
                    current,
                    delta, "Buffer counter underflowed. Clamping value to `0`."
                );
                0
            }))
        })
        .ok();
}

/// Snapshot of category metrics.
struct CategorySnapshot {
    event_count: u64,
    event_byte_size: u64,
}

impl CategorySnapshot {
    /// Returns `true` if any of the values are non-zero.
    fn has_updates(&self) -> bool {
        self.event_count > 0 || self.event_byte_size > 0
    }
}

/// Per-category metrics.
///
/// This tracks the number of events, and their size in the buffer, that a given category has interacted with. A
/// category in this case could be something like the receive or send categories i.e. being written into the buffer, and
/// then read out of the buffer. Overall, it's a simple grouping mechanism because we often want to track the change in
/// both number of events, and their size as measured by the buffer.
#[derive(Debug, Default)]
struct CategoryMetrics {
    event_count: AtomicU64,
    event_byte_size: AtomicU64,
}

impl CategoryMetrics {
    /// Increments the event count and byte size by the given amounts.
    fn increment(&self, event_count: u64, event_byte_size: u64) {
        increment_counter(&self.event_count, event_count);
        increment_counter(&self.event_byte_size, event_byte_size);
    }

    /// Decrements the event count and byte size by the given amounts.
    fn decrement(&self, event_count: u64, event_byte_size: u64) {
        decrement_counter(&self.event_count, event_count);
        decrement_counter(&self.event_byte_size, event_byte_size);
    }

    /// Sets the event count and event byte size to the given amount.
    ///
    /// Most updates are meant to be incremental, so this should be used sparingly.
    fn set(&self, event_count: u64, event_byte_size: u64) {
        self.event_count.store(event_count, ORDERING);
        self.event_byte_size.store(event_byte_size, ORDERING);
    }

    /// Gets a snapshot of the event count and event byte size.
    fn get(&self) -> CategorySnapshot {
        CategorySnapshot {
            event_count: self.event_count.load(ORDERING),
            event_byte_size: self.event_byte_size.load(ORDERING),
        }
    }

    /// Gets a snapshot of the event count and event byte size by "consuming" the values.
    ///
    /// This essentially resets both metrics while capturing their value at the time they were reset. This is useful if
    /// you want to only emit updates when values have been incremented/set to a non-zero value, as by consuming each
    /// time, you can tell if anything has changed since the last call to `consume` without needing internal state to
    /// track the last seen values.
    fn consume(&self) -> CategorySnapshot {
        CategorySnapshot {
            event_count: self.event_count.swap(0, ORDERING),
            event_byte_size: self.event_byte_size.swap(0, ORDERING),
        }
    }
}

/// Handle to buffer usage metrics for a specific buffer stage.
#[derive(Clone, Debug)]
pub struct BufferUsageHandle {
    state: Arc<BufferUsageData>,
}

impl BufferUsageHandle {
    /// Creates a no-op [`BufferUsageHandle`] handle.
    ///
    /// No usage data is written or stored.
    pub(crate) fn noop() -> Self {
        BufferUsageHandle {
            state: Arc::new(BufferUsageData::new(0)),
        }
    }

    /// Gets a snapshot of the buffer usage data, representing an instantaneous view of the different values.
    pub fn snapshot(&self) -> BufferUsageSnapshot {
        self.state.snapshot()
    }

    /// Sets the limits for this buffer component.
    ///
    /// Limits are exposed as gauges to provide stable values when superimposed on dashboards/graphs with the "actual"
    /// usage amounts.
    pub fn set_buffer_limits(&self, max_bytes: Option<u64>, max_events: Option<usize>) {
        let max_events = max_events
            .and_then(|n| u64::try_from(n).ok().or(Some(u64::MAX)))
            .unwrap_or(0);
        let max_bytes = max_bytes.unwrap_or(0);

        self.state.max_size.set(max_events, max_bytes);
    }

    /// Increments the number of events (and their total size) received by this buffer component.
    ///
    /// This represents the events being sent into the buffer.
    pub fn increment_received_event_count_and_byte_size(&self, count: u64, byte_size: u64) {
        if count > 0 || byte_size > 0 {
            self.state.received.increment(count, byte_size);
            self.state.current.increment(count, byte_size);
        }
    }

    /// Increments the number of events (and their total size) sent by this buffer component.
    ///
    /// This represents the events being read out of the buffer.
    pub fn increment_sent_event_count_and_byte_size(&self, count: u64, byte_size: u64) {
        if count > 0 || byte_size > 0 {
            self.state.sent.increment(count, byte_size);
            self.state.current.decrement(count, byte_size);
        }
    }

    /// Increment the number of dropped events (and their total size) for this buffer component.
    pub fn increment_dropped_event_count_and_byte_size(
        &self,
        count: u64,
        byte_size: u64,
        intentional: bool,
    ) {
        if count > 0 || byte_size > 0 {
            if intentional {
                self.state.dropped_intentional.increment(count, byte_size);
            } else {
                self.state.dropped.increment(count, byte_size);
            }
            self.state.current.decrement(count, byte_size);
        }
    }
}

#[derive(Debug, Default)]
struct BufferUsageData {
    idx: usize,
    received: CategoryMetrics,
    sent: CategoryMetrics,
    dropped: CategoryMetrics,
    dropped_intentional: CategoryMetrics,
    max_size: CategoryMetrics,
    current: CategoryMetrics,
}

impl BufferUsageData {
    fn new(idx: usize) -> Self {
        Self {
            idx,
            ..Default::default()
        }
    }

    fn snapshot(&self) -> BufferUsageSnapshot {
        let received = self.received.get();
        let sent = self.sent.get();
        let dropped = self.dropped.get();
        let dropped_intentional = self.dropped_intentional.get();
        let max_size = self.max_size.get();

        BufferUsageSnapshot {
            received_event_count: received.event_count,
            received_byte_size: received.event_byte_size,
            sent_event_count: sent.event_count,
            sent_byte_size: sent.event_byte_size,
            dropped_event_count: dropped.event_count,
            dropped_event_byte_size: dropped.event_byte_size,
            dropped_event_count_intentional: dropped_intentional.event_count,
            dropped_event_byte_size_intentional: dropped_intentional.event_byte_size,
            max_size_bytes: max_size.event_byte_size,
            max_size_events: max_size
                .event_count
                .try_into()
                .expect("should never be bigger than `usize`"),
        }
    }
}

/// Snapshot of buffer usage metrics.
#[derive(Debug)]
pub struct BufferUsageSnapshot {
    pub received_event_count: u64,
    pub received_byte_size: u64,
    pub sent_event_count: u64,
    pub sent_byte_size: u64,
    pub dropped_event_count: u64,
    pub dropped_event_byte_size: u64,
    pub dropped_event_count_intentional: u64,
    pub dropped_event_byte_size_intentional: u64,
    pub max_size_bytes: u64,
    pub max_size_events: usize,
}

/// Builder for tracking buffer usage metrics.
///
/// While building a buffer topology, `BufferUsage` can be utilized to create metrics storage for each individual buffer
/// stage. A handle is provided to allow each buffer stage to update their metrics from one or multiple locations, as
/// needed. Reporting of the metrics is handled centrally to keep buffer stages simpler and ensure consistent reporting.
pub struct BufferUsage {
    span: Span,
    stages: Vec<Arc<BufferUsageData>>,
}

impl BufferUsage {
    /// Creates an instance of [`BufferUsage`] attached to the given span.
    ///
    /// As buffers can have multiple stages, callers have the ability to register each stage via [`add_stage`].
    pub fn from_span(span: Span) -> BufferUsage {
        Self {
            span,
            stages: Vec::new(),
        }
    }

    /// Adds a new stage to track usage for.
    ///
    /// A [`BufferUsageHandle`] is returned that the caller can use to actually update the usage metrics with.  This
    /// handle will only update the usage metrics for the particular stage it was added for.
    pub fn add_stage(&mut self, idx: usize) -> BufferUsageHandle {
        let data = Arc::new(BufferUsageData::new(idx));
        let handle = BufferUsageHandle {
            state: Arc::clone(&data),
        };

        self.stages.push(data);
        handle
    }

    /// Installs a reporter for the configured stages which periodically reports buffer usage metrics.
    ///
    /// Metrics are reported every 2 seconds.
    ///
    /// The `buffer_id` should be a unique name -- ideally the `component_id` of the sink using this buffer -- but is
    /// not used for anything other than reporting, and so has no _requirement_ to be unique.
    pub fn install(self, buffer_id: &str) {
        let buffer_id = buffer_id.to_string();
        let span = self.span;
        let stages = self.stages;
        let task_name = format!("buffer usage reporter ({buffer_id})");

        let task = async move {
            let mut interval = interval(Duration::from_secs(2));
            loop {
                interval.tick().await;

                for stage in &stages {
                    let max_size = stage.max_size.get();
                    emit(BufferCreated {
                        buffer_id: buffer_id.clone(),
                        idx: stage.idx,
                        max_size_bytes: max_size.event_byte_size,
                        max_size_events: max_size
                            .event_count
                            .try_into()
                            .expect("should never be bigger than `usize`"),
                    });

                    let current = stage.current.get();
                    let received = stage.received.consume();
                    if received.has_updates() {
                        emit(BufferEventsReceived {
                            buffer_id: buffer_id.clone(),
                            idx: stage.idx,
                            count: received.event_count,
                            byte_size: received.event_byte_size,
                            total_count: current.event_count,
                            total_byte_size: current.event_byte_size,
                        });
                    }

                    let sent = stage.sent.consume();
                    if sent.has_updates() {
                        emit(BufferEventsSent {
                            buffer_id: buffer_id.clone(),
                            idx: stage.idx,
                            count: sent.event_count,
                            byte_size: sent.event_byte_size,
                            total_count: current.event_count,
                            total_byte_size: current.event_byte_size,
                        });
                    }

                    let dropped = stage.dropped.consume();
                    if dropped.has_updates() {
                        emit(BufferEventsDropped {
                            buffer_id: buffer_id.clone(),
                            idx: stage.idx,
                            intentional: false,
                            reason: "corrupted_events",
                            count: dropped.event_count,
                            byte_size: dropped.event_byte_size,
                            total_count: current.event_count,
                            total_byte_size: current.event_byte_size,
                        });
                    }

                    let dropped_intentional = stage.dropped_intentional.consume();
                    if dropped_intentional.has_updates() {
                        emit(BufferEventsDropped {
                            buffer_id: buffer_id.clone(),
                            idx: stage.idx,
                            intentional: true,
                            reason: "drop_newest",
                            count: dropped_intentional.event_count,
                            byte_size: dropped_intentional.event_byte_size,
                            total_count: current.event_count,
                            total_byte_size: current.event_byte_size,
                        });
                    }
                }
            }
        };

        spawn_named(task.instrument(span.or_current()), task_name.as_str());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_multithreaded_updates_are_correct() {
        const NUM_THREADS: u64 = 16;
        const INCREMENTS_PER_THREAD: u64 = 10_000;

        let counter = Arc::new(AtomicU64::new(0));

        let mut handles = vec![];

        for _ in 0..NUM_THREADS {
            let counter = Arc::clone(&counter);
            let handle = thread::spawn(move || {
                for _ in 0..INCREMENTS_PER_THREAD {
                    increment_counter(&counter, 1);
                    decrement_counter(&counter, 1);
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(counter.load(ORDERING), 0);
    }

    #[test]
    fn test_decrement_counter_prevents_negatives() {
        let counter = AtomicU64::new(100);

        decrement_counter(&counter, 50);
        assert_eq!(counter.load(ORDERING), 50);

        decrement_counter(&counter, 100);
        assert_eq!(counter.load(ORDERING), 0);

        decrement_counter(&counter, 50);
        assert_eq!(counter.load(ORDERING), 0);

        decrement_counter(&counter, u64::MAX);
        assert_eq!(counter.load(ORDERING), 0);
    }

    #[test]
    fn test_increment_counter_prevents_overflow() {
        let counter = AtomicU64::new(u64::MAX - 2);

        increment_counter(&counter, 1);
        assert_eq!(counter.load(ORDERING), u64::MAX - 1);

        increment_counter(&counter, 1);
        assert_eq!(counter.load(ORDERING), u64::MAX);

        increment_counter(&counter, 1);
        assert_eq!(counter.load(ORDERING), u64::MAX);

        increment_counter(&counter, u64::MAX);
        assert_eq!(counter.load(ORDERING), u64::MAX);
    }
}
