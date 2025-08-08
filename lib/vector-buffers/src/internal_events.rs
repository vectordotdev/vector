use std::time::Duration;

use crate::cast_utils::u64_to_f64_safe;
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

        let labels = vec![
            ("buffer_id".to_string(), self.buffer_id),
            ("intentional".to_string(), intentional_str.to_string()),
        ];

        counter!("buffer_discarded_events_total", &labels).increment(self.count);

        if self.byte_size > 0 {
            counter!("buffer_discarded_bytes_total", &labels).increment(self.byte_size);
        }
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
