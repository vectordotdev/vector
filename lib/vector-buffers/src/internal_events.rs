use std::time::Duration;

use metrics::{counter, gauge, histogram, Histogram};
use vector_common::{
    internal_event::{error_type, InternalEvent},
    registered_event,
};

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
        gauge!("buffer_events", "stage" => self.idx.to_string()).increment(self.count as f64);
        gauge!("buffer_byte_size", "stage" => self.idx.to_string())
            .increment(self.byte_size as f64);
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
        gauge!("buffer_events", "stage" => self.idx.to_string()).decrement(self.count as f64);
        gauge!("buffer_byte_size", "stage" => self.idx.to_string())
            .decrement(self.byte_size as f64);
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
        gauge!("buffer_events", "stage" => self.idx.to_string()).decrement(self.count as f64);
        gauge!("buffer_byte_size", "stage" => self.idx.to_string())
            .decrement(self.byte_size as f64);
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
            internal_log_rate_limit = true,
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
