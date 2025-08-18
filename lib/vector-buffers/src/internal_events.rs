use dashmap::DashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::LazyLock;
use std::time::Duration;

use crate::cast_utils::{i64_to_f64_safe, u64_to_f64_safe};
use metrics::{counter, gauge, histogram, Histogram};
use vector_common::{
    internal_event::{error_type, InternalEvent},
    registered_event,
};

static BUFFER_COUNTERS: LazyLock<DashMap<(String, usize), (AtomicI64, AtomicI64)>> =
    LazyLock::new(DashMap::new);

fn update_and_get(counter: &AtomicI64, delta: i64) -> i64 {
    let mut new_val = 0;
    counter
        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |current| {
            let updated = current.saturating_add(delta).clamp(0, i64::MAX);
            new_val = updated;
            Some(updated)
        })
        .ok();
    new_val
}

fn update_buffer_gauge(buffer_id: &str, stage: usize, events_delta: i64, bytes_delta: i64) {
    let counters = BUFFER_COUNTERS
        .entry((buffer_id.to_string(), stage))
        .or_insert_with(|| (AtomicI64::new(0), AtomicI64::new(0)));

    let new_events = update_and_get(&counters.0, events_delta);
    let new_bytes = update_and_get(&counters.1, bytes_delta);

    gauge!("buffer_events",
        "buffer_id" => buffer_id.to_string(),
        "stage" => stage.to_string()
    )
    .set(i64_to_f64_safe(new_events));

    gauge!("buffer_byte_size",
        "buffer_id" => buffer_id.to_string(),
        "stage" => stage.to_string()
    )
    .set(i64_to_f64_safe(new_bytes));
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
    pub count: u64,
    pub byte_size: u64,
}

impl InternalEvent for BufferEventsReceived {
    fn emit(self) {
        counter!("buffer_received_events_total",
            "buffer_id" => self.buffer_id.clone(),
            "stage" => self.idx.to_string()
        )
        .increment(self.count);

        counter!("buffer_received_bytes_total",
            "buffer_id" => self.buffer_id.clone(),
            "stage" => self.idx.to_string()
        )
        .increment(self.byte_size);

        let count_delta = i64::try_from(self.count).unwrap_or(i64::MAX);
        let bytes_delta = i64::try_from(self.byte_size).unwrap_or(i64::MAX);
        update_buffer_gauge(&self.buffer_id, self.idx, count_delta, bytes_delta);
    }
}

pub struct BufferEventsSent {
    pub buffer_id: String,
    pub idx: usize,
    pub count: u64,
    pub byte_size: u64,
}

impl InternalEvent for BufferEventsSent {
    fn emit(self) {
        counter!("buffer_sent_events_total",
            "buffer_id" => self.buffer_id.clone(),
            "stage" => self.idx.to_string()
        )
        .increment(self.count);

        counter!("buffer_sent_bytes_total",
            "buffer_id" => self.buffer_id.clone(),
            "stage" => self.idx.to_string())
        .increment(self.byte_size);

        let count_delta = i64::try_from(self.count).unwrap_or(i64::MAX);
        let bytes_delta = i64::try_from(self.byte_size).unwrap_or(i64::MAX);
        update_buffer_gauge(&self.buffer_id, self.idx, -count_delta, -bytes_delta);
    }
}

pub struct BufferEventsDropped {
    pub buffer_id: String,
    pub idx: usize,
    pub count: u64,
    pub byte_size: u64,
    pub intentional: bool,
    pub reason: &'static str,
}

impl InternalEvent for BufferEventsDropped {
    fn emit(self) {
        let intentional_str = if self.intentional { "true" } else { "false" };
        if self.intentional {
            debug!(
                message = "Events dropped.",
                count = %self.count,
                intentional = %intentional_str,
                reason = %self.reason,
                buffer_id = %self.buffer_id,
                stage = %self.idx,
            );
        } else {
            error!(
                message = "Events dropped.",
                count = %self.count,
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
        .increment(self.count);

        let count_delta = i64::try_from(self.count).unwrap_or(i64::MAX);
        let bytes_delta = i64::try_from(self.byte_size).unwrap_or(i64::MAX);

        update_buffer_gauge(&self.buffer_id, self.idx, -count_delta, -bytes_delta);
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

    fn get_counter_values(buffer_id: &str, stage: usize) -> (i64, i64) {
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
        updates: &[(i64, i64)],
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
                update_buffer_gauge(buffer_id, stage, *events_delta, *bytes_delta);
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

        update_buffer_gauge("test_buffer", 0, 10, 1024);
        let (events, bytes) = get_counter_values("test_buffer", 0);
        assert_eq!(events, 10);
        assert_eq!(bytes, 1024);
    }

    #[test]
    fn test_increment_and_decrement() {
        let _guard = TEST_LOCK.lock().unwrap();
        reset_counters();

        update_buffer_gauge("test_buffer", 1, 100, 2048);
        update_buffer_gauge("test_buffer", 1, -50, -1024);
        let (events, bytes) = get_counter_values("test_buffer", 1);
        assert_eq!(events, 50);
        assert_eq!(bytes, 1024);
    }

    #[test]
    fn test_decrement_below_zero_clamped_to_zero() {
        let _guard = TEST_LOCK.lock().unwrap();
        reset_counters();

        update_buffer_gauge("test_buffer", 2, 5, 100);
        update_buffer_gauge("test_buffer", 2, -10, -200);
        let (events, bytes) = get_counter_values("test_buffer", 2);

        assert_eq!(events, 0);
        assert_eq!(bytes, 0);
    }

    #[test]
    fn test_multiple_stages_are_independent() {
        let _guard = TEST_LOCK.lock().unwrap();
        reset_counters();

        update_buffer_gauge("test_buffer", 0, 10, 100);
        update_buffer_gauge("test_buffer", 1, 20, 200);
        let (events0, bytes0) = get_counter_values("test_buffer", 0);
        let (events1, bytes1) = get_counter_values("test_buffer", 1);
        assert_eq!(events0, 10);
        assert_eq!(bytes0, 100);
        assert_eq!(events1, 20);
        assert_eq!(bytes1, 200);
    }

    #[test]
    fn test_multithreaded_updates_are_correct() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        reset_counters();

        let num_threads = 10;
        let increments_per_thread = 1000;
        let mut handles = vec![];

        for _ in 0..num_threads {
            let handle = thread::spawn(move || {
                for _ in 0..increments_per_thread {
                    update_buffer_gauge("test_buffer", 0, 1, 10);
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        let (final_events, final_bytes) = get_counter_values("test_buffer", 0);
        let expected_events = i64::from(num_threads * increments_per_thread);
        let expected_bytes = i64::from(num_threads * increments_per_thread * 10);

        assert_eq!(final_events, expected_events);
        assert_eq!(final_bytes, expected_bytes);
    }

    #[test]
    fn test_large_values_capped_to_f64_safe_max() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        reset_counters();

        update_buffer_gauge("test_buffer", 3, F64_SAFE_INT_MAX * 2, F64_SAFE_INT_MAX * 2);

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
    fn test_should_not_be_negative_with_recorder() {
        assert_gauge_state("test_buffer", 1, &[(100, 1024), (-200, -4096)], 0.0, 0.0);
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
