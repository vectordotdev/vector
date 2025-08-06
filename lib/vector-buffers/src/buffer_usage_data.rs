use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, LazyLock,
    },
    time::Duration,
};

use dashmap::DashMap;
use tokio::time::interval;
use tracing::{Instrument, Span};
use vector_common::internal_event::emit;

use crate::{
    internal_events::{BufferCreated, BufferEventsDropped, BufferEventsReceived, BufferEventsSent},
    spawn_named,
};

static BUFFER_COUNTERS: LazyLock<DashMap<(String, usize), (AtomicU64, AtomicU64)>> =
    LazyLock::new(DashMap::new);

fn update_buffer_counters(
    buffer_id: &str,
    stage: usize,
    events_delta: u64,
    bytes_delta: u64,
) -> (u64, u64) {
    fn update_and_get(counter: &AtomicU64, delta: u64) -> u64 {
        let mut new_val = 0;
        counter
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |current| {
                let updated = current.saturating_add(delta);
                new_val = updated;
                Some(updated)
            })
            .ok();
        new_val
    }

    let counters = BUFFER_COUNTERS
        .entry((buffer_id.to_string(), stage))
        .or_insert_with(|| (AtomicU64::new(0), AtomicU64::new(0)));

    let new_events = update_and_get(&counters.0, events_delta);
    let new_bytes = update_and_get(&counters.1, bytes_delta);

    (new_events, new_bytes)
}

/// Snapshot of category metrics.
struct CategorySnapshot {
    delta_count: u64,
    delta_byte_size: u64,
}

impl CategorySnapshot {
    /// Returns `true` if any of the values are non-zero.
    fn has_updates(&self) -> bool {
        self.delta_count > 0 || self.delta_byte_size > 0
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
    delta_count: AtomicU64,
    delta_byte_size: AtomicU64,
}

impl CategoryMetrics {
    /// Increments the event count and byte size by the given amounts.
    fn increment(&self, event_count: u64, event_byte_size: u64) {
        self.delta_count.fetch_add(event_count, Ordering::Relaxed);
        self.delta_byte_size
            .fetch_add(event_byte_size, Ordering::Relaxed);
    }

    /// Sets the event count and event byte size to the given amount.
    ///
    /// Most updates are meant to be incremental, so this should be used sparingly.
    fn set(&self, event_count: u64, event_byte_size: u64) {
        self.delta_count.store(event_count, Ordering::Release);
        self.delta_byte_size
            .store(event_byte_size, Ordering::Release);
    }

    /// Gets a snapshot of the event count and event byte size.
    fn get(&self) -> CategorySnapshot {
        CategorySnapshot {
            delta_count: self.delta_count.load(Ordering::Acquire),
            delta_byte_size: self.delta_byte_size.load(Ordering::Acquire),
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
            delta_count: self.delta_count.swap(0, Ordering::AcqRel),
            delta_byte_size: self.delta_byte_size.swap(0, Ordering::AcqRel),
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
        self.state.received.increment(count, byte_size);
    }

    /// Increments the number of events (and their total size) sent by this buffer component.
    ///
    /// This represents the events being read out of the buffer.
    pub fn increment_sent_event_count_and_byte_size(&self, count: u64, byte_size: u64) {
        self.state.sent.increment(count, byte_size);
    }

    /// Increment the number of dropped events (and their total size) for this buffer component.
    pub fn increment_dropped_event_count_and_byte_size(
        &self,
        count: u64,
        byte_size: u64,
        intentional: bool,
    ) {
        if intentional {
            self.state.dropped_intentional.increment(count, byte_size);
        } else {
            self.state.dropped.increment(count, byte_size);
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
            received_event_count: received.delta_count,
            received_byte_size: received.delta_byte_size,
            sent_event_count: sent.delta_count,
            sent_byte_size: sent.delta_byte_size,
            dropped_event_count: dropped.delta_count,
            dropped_event_byte_size: dropped.delta_byte_size,
            dropped_event_count_intentional: dropped_intentional.delta_count,
            dropped_event_byte_size_intentional: dropped_intentional.delta_byte_size,
            max_size_bytes: max_size.delta_byte_size,
            max_size_events: max_size
                .delta_count
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
                        idx: stage.idx,
                        max_size_bytes: max_size.delta_byte_size,
                        max_size_events: max_size
                            .delta_count
                            .try_into()
                            .expect("should never be bigger than `usize`"),
                    });

                    let received = stage.received.consume();
                    if received.has_updates() {
                        let (total_count, total_byte_size) = update_buffer_counters(
                            &buffer_id,
                            stage.idx,
                            received.delta_count,
                            received.delta_byte_size,
                        );
                        emit(BufferEventsReceived {
                            buffer_id: buffer_id.clone(),
                            idx: stage.idx,
                            delta_count: received.delta_count,
                            delta_byte_size: received.delta_byte_size,
                            total_count,
                            total_byte_size,
                        });
                    }

                    let sent = stage.sent.consume();
                    if sent.has_updates() {
                        let (total_count, total_byte_size) = update_buffer_counters(
                            &buffer_id,
                            stage.idx,
                            received.delta_count,
                            received.delta_byte_size,
                        );
                        emit(BufferEventsSent {
                            buffer_id: buffer_id.clone(),
                            idx: stage.idx,
                            delta_count: sent.delta_count,
                            delta_byte_size: sent.delta_byte_size,
                            total_count,
                            total_byte_size,
                        });
                    }

                    let dropped = stage.dropped.consume();
                    if dropped.has_updates() {
                        let (total_count, total_byte_size) = update_buffer_counters(
                            &buffer_id,
                            stage.idx,
                            received.delta_count,
                            received.delta_byte_size,
                        );
                        emit(BufferEventsDropped {
                            buffer_id: buffer_id.clone(),
                            idx: stage.idx,
                            intentional: false,
                            reason: "corrupted_events",
                            delta_count: dropped.delta_count,
                            delta_byte_size: dropped.delta_byte_size,
                            total_count,
                            total_byte_size,
                        });
                    }

                    let dropped_intentional = stage.dropped_intentional.consume();
                    if dropped_intentional.has_updates() {
                        let (total_count, total_byte_size) = update_buffer_counters(
                            &buffer_id,
                            stage.idx,
                            received.delta_count,
                            received.delta_byte_size,
                        );
                        emit(BufferEventsDropped {
                            buffer_id: buffer_id.clone(),
                            idx: stage.idx,
                            intentional: true,
                            reason: "drop_newest",
                            delta_count: dropped_intentional.delta_count,
                            delta_byte_size: dropped_intentional.delta_byte_size,
                            total_count,
                            total_byte_size,
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
    use crate::cast_utils::F64_SAFE_INT_MAX;
    use std::sync::Mutex;
    use std::thread;

    static TEST_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    fn reset_counters() {
        BUFFER_COUNTERS.clear();
    }

    fn get_counter_values(buffer_id: &str, stage: usize) -> (u64, u64) {
        match BUFFER_COUNTERS.get(&(buffer_id.to_string(), stage)) {
            Some(counters) => {
                let events = counters.0.load(Ordering::Relaxed);
                let bytes = counters.1.load(Ordering::Relaxed);
                (events, bytes)
            }
            None => (0, 0),
        }
    }

    #[test]
    fn test_increment() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        reset_counters();

        update_buffer_counters("test_buffer", 0, 10, 1024);
        let (events, bytes) = get_counter_values("test_buffer", 0);

        assert_eq!(events, 10);
        assert_eq!(bytes, 1024);
    }

    #[test]
    fn test_multiple_stages_are_independent() {
        let _guard = TEST_LOCK.lock().unwrap();
        reset_counters();

        update_buffer_counters("test_buffer", 0, 10, 100);
        update_buffer_counters("test_buffer", 1, 20, 200);
        let (events0, bytes0) = get_counter_values("test_buffer", 0);
        let (events1, bytes1) = get_counter_values("test_buffer", 1);
        assert_eq!(events0, 10);
        assert_eq!(bytes0, 100);
        assert_eq!(events1, 20);
        assert_eq!(bytes1, 200);
    }

    #[test]
    fn test_multithreaded_updates_are_correct() {
        const NUM_THREADS: u64 = 10;
        const INCREMENTS_PER_THREAD: u64 = 1000;
        const EXPECTED_EVENTS: u64 = NUM_THREADS * INCREMENTS_PER_THREAD;
        const EXPECTED_BYTES: u64 = NUM_THREADS * INCREMENTS_PER_THREAD * 10;

        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        reset_counters();

        let mut handles = vec![];

        for _ in 0..NUM_THREADS {
            let handle = thread::spawn(move || {
                for _ in 0..INCREMENTS_PER_THREAD {
                    update_buffer_counters("test_buffer", 0, 1, 10);
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        let (final_events, final_bytes) = get_counter_values("test_buffer", 0);

        assert_eq!(final_events, EXPECTED_EVENTS);
        assert_eq!(final_bytes, EXPECTED_BYTES);
    }

    #[test]
    fn test_large_values_capped_to_f64_safe_max() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        reset_counters();

        update_buffer_counters("test_buffer", 3, F64_SAFE_INT_MAX * 2, F64_SAFE_INT_MAX * 2);

        let (events, bytes) = get_counter_values("test_buffer", 3);

        assert!(events > F64_SAFE_INT_MAX);
        assert!(bytes > F64_SAFE_INT_MAX);

        let capped_events = events.min(F64_SAFE_INT_MAX);
        let capped_bytes = bytes.min(F64_SAFE_INT_MAX);

        assert_eq!(capped_events, F64_SAFE_INT_MAX);
        assert_eq!(capped_bytes, F64_SAFE_INT_MAX);
    }
}
