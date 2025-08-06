use dashmap::DashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::LazyLock;
use std::time::Duration;

use crate::cast_utils::u64_to_f64_safe;
use metrics::{counter, gauge, histogram, Histogram};
use vector_common::{
    internal_event::{error_type, InternalEvent},
    registered_event,
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

fn emit_buffer_gauge(buffer_id: &str, stage: usize, new_events: u64, new_bytes: u64) {
    gauge!("buffer_events",
        "buffer_id" => buffer_id.to_string(),
        "stage" => stage.to_string()
    )
    .set(u64_to_f64_safe(new_events));

    gauge!("buffer_byte_size",
        "buffer_id" => buffer_id.to_string(),
        "stage" => stage.to_string()
    )
    .set(u64_to_f64_safe(new_bytes));
}

pub struct BufferCreated {
    pub idx: usize,
    pub max_size_events: usize,
    pub max_size_bytes: u64,
}

impl InternalEvent for BufferCreated {
    fn emit(self) {
        if self.max_size_events != 0 {
            gauge!("buffer_max_event_size", "stage" => self.idx.to_string())
                .set(u64_to_f64_safe(self.max_size_events as u64));
        }
        if self.max_size_bytes != 0 {
            gauge!("buffer_max_byte_size", "stage" => self.idx.to_string())
                .set(u64_to_f64_safe(self.max_size_bytes));
        }
    }
}

pub struct BufferEventsReceived {
    pub buffer_id: String,
    pub idx: usize,
    pub delta_count: u64,
    pub delta_byte_size: u64,
}

impl InternalEvent for BufferEventsReceived {
    fn emit(self) {
        counter!("buffer_received_events_total",
            "buffer_id" => self.buffer_id.clone(),
            "stage" => self.idx.to_string()
        )
        .increment(self.delta_count);

        counter!("buffer_received_bytes_total",
            "buffer_id" => self.buffer_id.clone(),
            "stage" => self.idx.to_string()
        )
        .increment(self.delta_byte_size);

        let (new_events, new_bytes) = update_buffer_counters(
            &self.buffer_id,
            self.idx,
            self.delta_count,
            self.delta_byte_size,
        );
        emit_buffer_gauge(&self.buffer_id, self.idx, new_events, new_bytes);
    }
}

pub struct BufferEventsSent {
    pub buffer_id: String,
    pub idx: usize,
    pub delta_count: u64,
    pub delta_byte_size: u64,
}

impl InternalEvent for BufferEventsSent {
    fn emit(self) {
        counter!("buffer_sent_events_total",
            "buffer_id" => self.buffer_id.clone(),
            "stage" => self.idx.to_string()
        )
        .increment(self.delta_count);

        counter!("buffer_sent_bytes_total",
            "buffer_id" => self.buffer_id.clone(),
            "stage" => self.idx.to_string())
        .increment(self.delta_byte_size);

        let (new_events, new_bytes) = update_buffer_counters(
            &self.buffer_id,
            self.idx,
            self.delta_count,
            self.delta_byte_size,
        );
        emit_buffer_gauge(&self.buffer_id, self.idx, new_events, new_bytes);
    }
}

pub struct BufferEventsDropped {
    pub buffer_id: String,
    pub idx: usize,
    pub delta_count: u64,
    pub delta_byte_size: u64,
    pub intentional: bool,
    pub reason: &'static str,
}

impl InternalEvent for BufferEventsDropped {
    fn emit(self) {
        let intentional_str = if self.intentional { "true" } else { "false" };
        if self.intentional {
            debug!(
                message = "Events dropped.",
                count = %self.delta_count,
                intentional = %intentional_str,
                reason = %self.reason,
                buffer_id = %self.buffer_id,
                stage = %self.idx,
            );
        } else {
            error!(
                message = "Events dropped.",
                count = %self.delta_count,
                intentional = %intentional_str,
                reason = %self.reason,
                buffer_id = %self.buffer_id,
                stage = %self.idx,
            );
        }

        counter!(
            "buffer_discarded_events_total",
            "buffer_id" => self.buffer_id.clone(),
            "intentional" => intentional_str,
        )
        .increment(self.delta_count);

        let (new_events, new_bytes) = update_buffer_counters(
            &self.buffer_id,
            self.idx,
            self.delta_count,
            self.delta_byte_size,
        );
        emit_buffer_gauge(&self.buffer_id, self.idx, new_events, new_bytes);
    }
}

pub struct BufferReadError {
    pub error_code: &'static str,
    pub error: String,
}

impl InternalEvent for BufferReadError {
    fn emit(self) {
        error!(
            message = "Error encountered during buffer read.",
            error = %self.error,
            error_code = self.error_code,
            error_type = error_type::READER_FAILED,
            stage = "processing",
        );
        counter!(
            "buffer_errors_total", "error_code" => self.error_code,
            "error_type" => "reader_failed",
            "stage" => "processing",
        )
        .increment(1);
    }
}

registered_event! {
    BufferSendDuration {
        stage: usize,
    } => {
        send_duration: Histogram = histogram!("buffer_send_duration_seconds", "stage" => self.stage.to_string()),
    }

    fn emit(&self, duration: Duration) {
        self.send_duration.record(duration);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cast_utils::F64_SAFE_INT_MAX;
    use metrics::{Key, Label};
    use metrics_util::debugging::{DebugValue, DebuggingRecorder};
    use metrics_util::{CompositeKey, MetricKind};
    use ordered_float::OrderedFloat;
    use std::borrow::Cow;
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

    fn assert_gauge_state(
        buffer_id: &str,
        stage: usize,
        updates: &[(u64, u64)],
        expected_events: f64,
        expected_bytes: f64,
    ) {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        reset_counters();

        let recorder = DebuggingRecorder::default();
        let snapshotter = recorder.snapshotter();

        metrics::with_local_recorder(&recorder, move || {
            for (events_delta, bytes_delta) in updates {
                let (new_events, new_bytes) =
                    update_buffer_counters(buffer_id, stage, *events_delta, *bytes_delta);
                emit_buffer_gauge(buffer_id, stage, new_events, new_bytes);
            }

            let metrics = snapshotter.snapshot().into_vec();

            let buffer_id_cow: Cow<'static, str> = Cow::Owned(buffer_id.to_string());
            let buffer_id_label = Label::new("buffer_id", buffer_id_cow);

            let stage_label = Label::new("stage", stage.to_string());

            let expected_metrics = vec![
                (
                    CompositeKey::new(
                        MetricKind::Gauge,
                        Key::from_parts(
                            "buffer_events",
                            vec![buffer_id_label.clone(), stage_label.clone()],
                        ),
                    ),
                    None,
                    None,
                    DebugValue::Gauge(OrderedFloat(expected_events)),
                ),
                (
                    CompositeKey::new(
                        MetricKind::Gauge,
                        Key::from_parts(
                            "buffer_byte_size",
                            vec![buffer_id_label.clone(), stage_label],
                        ),
                    ),
                    None,
                    None,
                    DebugValue::Gauge(OrderedFloat(expected_bytes)),
                ),
            ];

            // Compare metrics without needing to sort if order doesn't matter
            assert_eq!(metrics.len(), expected_metrics.len());
            for expected in &expected_metrics {
                assert!(
                    metrics.contains(expected),
                    "Missing expected metric: {expected:?}"
                );
            }
        });
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

    #[test]
    fn test_increment_with_recorder() {
        assert_gauge_state("test_buffer", 0, &[(100, 2048), (200, 1024)], 300.0, 3072.0);
    }

    #[test]
    fn test_increment_with_custom_buffer_id() {
        assert_gauge_state(
            "buffer_alpha",
            0,
            &[(100, 2048), (200, 1024)],
            300.0,
            3072.0,
        );
    }

    #[test]
    fn test_increment_with_another_buffer_id() {
        assert_gauge_state("buffer_beta", 0, &[(10, 100), (5, 50)], 15.0, 150.0);
    }
}
