use dashmap::DashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::LazyLock;
use std::time::Duration;

use metrics::{counter, gauge, histogram, Histogram};
use vector_common::{
    internal_event::{error_type, InternalEvent},
    registered_event,
};

static BUFFER_COUNTERS: LazyLock<DashMap<usize, (AtomicI64, AtomicI64)>> =
    LazyLock::new(DashMap::new);

#[allow(clippy::cast_precision_loss)]
fn update_buffer_gauge(stage: usize, events_delta: i64, bytes_delta: i64) {
    let entry = BUFFER_COUNTERS
        .entry(stage)
        .or_insert_with(|| (AtomicI64::new(0), AtomicI64::new(0)));

    let (events, bytes) = entry.value();

    let new_events = (events.fetch_add(events_delta, Ordering::SeqCst) + events_delta).max(0);
    let new_bytes = (bytes.fetch_add(bytes_delta, Ordering::SeqCst) + bytes_delta).max(0);

    gauge!("buffer_events", "stage" => stage.to_string()).set(new_events as f64);
    gauge!("buffer_byte_size", "stage" => stage.to_string()).set(new_bytes as f64);
}

pub struct BufferCreated {
    pub idx: usize,
    pub max_size_events: usize,
    pub max_size_bytes: u64,
}

impl InternalEvent for BufferCreated {
    #[allow(clippy::cast_precision_loss)]
    fn emit(self) {
        if self.max_size_events != 0 {
            gauge!("buffer_max_event_size", "stage" => self.idx.to_string())
                .set(self.max_size_events as f64);
        }
        if self.max_size_bytes != 0 {
            gauge!("buffer_max_byte_size", "stage" => self.idx.to_string())
                .set(self.max_size_bytes as f64);
        }
    }
}

pub struct BufferEventsReceived {
    pub idx: usize,
    pub count: u64,
    pub byte_size: u64,
}

impl InternalEvent for BufferEventsReceived {
    #[allow(clippy::cast_precision_loss)]
    fn emit(self) {
        counter!("buffer_received_events_total", "stage" => self.idx.to_string())
            .increment(self.count);
        counter!("buffer_received_bytes_total", "stage" => self.idx.to_string())
            .increment(self.byte_size);

        let count_delta = i64::try_from(self.count).unwrap_or(i64::MAX);
        let bytes_delta = i64::try_from(self.byte_size).unwrap_or(i64::MAX);
        update_buffer_gauge(self.idx, count_delta, bytes_delta);
    }
}

pub struct BufferEventsSent {
    pub idx: usize,
    pub count: u64,
    pub byte_size: u64,
}

impl InternalEvent for BufferEventsSent {
    #[allow(clippy::cast_precision_loss)]
    fn emit(self) {
        counter!("buffer_sent_events_total", "stage" => self.idx.to_string()).increment(self.count);
        counter!("buffer_sent_bytes_total", "stage" => self.idx.to_string())
            .increment(self.byte_size);

        let count_delta = i64::try_from(self.count).unwrap_or(i64::MAX);
        let bytes_delta = i64::try_from(self.byte_size).unwrap_or(i64::MAX);
        update_buffer_gauge(self.idx, -count_delta, -bytes_delta);
    }
}

pub struct BufferEventsDropped {
    pub idx: usize,
    pub count: u64,
    pub byte_size: u64,
    pub intentional: bool,
    pub reason: &'static str,
}

impl InternalEvent for BufferEventsDropped {
    #[allow(clippy::cast_precision_loss)]
    fn emit(self) {
        let intentional_str = if self.intentional { "true" } else { "false" };
        if self.intentional {
            debug!(
                message = "Events dropped.",
                count = %self.count,
                intentional = %intentional_str,
                reason = %self.reason,
                stage = %self.idx,
            );
        } else {
            error!(
                message = "Events dropped.",
                count = %self.count,
                intentional = %intentional_str,
                reason = %self.reason,
                stage = %self.idx,
            );
        }
        counter!(
            "buffer_discarded_events_total", "intentional" => intentional_str,
        )
        .increment(self.count);

        let count_delta = i64::try_from(self.count).unwrap_or(i64::MAX);
        let bytes_delta = i64::try_from(self.byte_size).unwrap_or(i64::MAX);
        update_buffer_gauge(self.idx, -count_delta, -bytes_delta);
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
    use std::sync::Mutex;
    use super::*;
    use std::thread;

    static TEST_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    fn reset_counters() {
        BUFFER_COUNTERS.clear();
    }

    fn get_counter_values(stage: usize) -> (i64, i64) {
        match BUFFER_COUNTERS.get(&stage) {
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
        let _guard = TEST_LOCK.lock().unwrap();
        reset_counters();

        update_buffer_gauge(0, 10, 1024);
        let (events, bytes) = get_counter_values(0);
        assert_eq!(events, 10);
        assert_eq!(bytes, 1024);
    }

    #[test]
    fn test_increment_and_decrement() {
        let _guard = TEST_LOCK.lock().unwrap();
        reset_counters();

        update_buffer_gauge(1, 100, 2048);
        update_buffer_gauge(1, -50, -1024);
        let (events, bytes) = get_counter_values(1);
        assert_eq!(events, 50);
        assert_eq!(bytes, 1024);
    }

    #[test]
    fn test_internal_counter_can_be_negative() {
        let _guard = TEST_LOCK.lock().unwrap();
        reset_counters();

        update_buffer_gauge(2, 5, 100);
        update_buffer_gauge(2, -10, -200);
        let (events, bytes) = get_counter_values(2);
        assert_eq!(events, -5);
        assert_eq!(bytes, -100);
    }

    #[test]
    fn test_multiple_stages_are_independent() {
        let _guard = TEST_LOCK.lock().unwrap();
        reset_counters();

        update_buffer_gauge(0, 10, 100);
        update_buffer_gauge(1, 20, 200);
        let (events0, bytes0) = get_counter_values(0);
        let (events1, bytes1) = get_counter_values(1);
        assert_eq!(events0, 10);
        assert_eq!(bytes0, 100);
        assert_eq!(events1, 20);
        assert_eq!(bytes1, 200);
    }

    #[test]
    fn test_multithreaded_updates_are_correct() {
        let _guard = TEST_LOCK.lock().unwrap();
        reset_counters();

        let num_threads = 10;
        let increments_per_thread = 1000;
        let mut handles = vec![];

        for _ in 0..num_threads {
            let handle = thread::spawn(move || {
                for _ in 0..increments_per_thread {
                    update_buffer_gauge(0, 1, 10);
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        let (final_events, final_bytes) = get_counter_values(0);
        let expected_events = i64::from(num_threads * increments_per_thread);
        let expected_bytes = i64::from(num_threads * increments_per_thread * 10);

        assert_eq!(final_events, expected_events);
        assert_eq!(final_bytes, expected_bytes);
    }
}
