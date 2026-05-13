use std::time::Duration;

use metrics::Histogram;
use vector_common::NamedInternalEvent;
use vector_common::{
    counter, gauge, histogram,
    internal_event::{CounterName, GaugeName, HistogramName, InternalEvent, error_type},
    registered_event,
};

#[derive(NamedInternalEvent)]
pub struct BufferCreated {
    pub buffer_id: String,
    pub idx: usize,
    pub max_size_events: usize,
    pub max_size_bytes: u64,
}

impl InternalEvent for BufferCreated {
    #[expect(clippy::cast_precision_loss)]
    fn emit(self) {
        let stage = self.idx.to_string();
        if self.max_size_events != 0 {
            gauge!(
                GaugeName::BufferMaxSizeEvents,
                "buffer_id" => self.buffer_id.clone(),
                "stage" => stage.clone(),
            )
            .set(self.max_size_events as f64);
            // DEPRECATED: buffer-bytes-events-metrics
            gauge!(
                GaugeName::BufferMaxEventSize,
                "buffer_id" => self.buffer_id.clone(),
                "stage" => stage.clone(),
            )
            .set(self.max_size_events as f64);
        }
        if self.max_size_bytes != 0 {
            gauge!(
                GaugeName::BufferMaxSizeBytes,
                "buffer_id" => self.buffer_id.clone(),
                "stage" => stage.clone(),
            )
            .set(self.max_size_bytes as f64);
            // DEPRECATED: buffer-bytes-events-metrics
            gauge!(
                GaugeName::BufferMaxByteSize,
                "buffer_id" => self.buffer_id,
                "stage" => stage,
            )
            .set(self.max_size_bytes as f64);
        }
    }
}

#[derive(NamedInternalEvent)]
pub struct BufferEventsReceived {
    pub buffer_id: String,
    pub idx: usize,
    pub count: u64,
    pub byte_size: u64,
    pub total_count: u64,
    pub total_byte_size: u64,
}

impl InternalEvent for BufferEventsReceived {
    #[expect(clippy::cast_precision_loss)]
    fn emit(self) {
        counter!(
            CounterName::BufferReceivedEventsTotal,
            "buffer_id" => self.buffer_id.clone(),
            "stage" => self.idx.to_string()
        )
        .increment(self.count);

        counter!(
            CounterName::BufferReceivedBytesTotal,
            "buffer_id" => self.buffer_id.clone(),
            "stage" => self.idx.to_string()
        )
        .increment(self.byte_size);
        // DEPRECATED: buffer-bytes-events-metrics
        gauge!(
            GaugeName::BufferEvents,
            "buffer_id" => self.buffer_id.clone(),
            "stage" => self.idx.to_string()
        )
        .set(self.total_count as f64);
        gauge!(
            GaugeName::BufferSizeEvents,
            "buffer_id" => self.buffer_id.clone(),
            "stage" => self.idx.to_string()
        )
        .set(self.total_count as f64);
        gauge!(
            GaugeName::BufferSizeBytes,
            "buffer_id" => self.buffer_id.clone(),
            "stage" => self.idx.to_string()
        )
        .set(self.total_byte_size as f64);
        // DEPRECATED: buffer-bytes-events-metrics
        gauge!(
            GaugeName::BufferByteSize,
            "buffer_id" => self.buffer_id,
            "stage" => self.idx.to_string()
        )
        .set(self.total_byte_size as f64);
    }
}

#[derive(NamedInternalEvent)]
pub struct BufferEventsSent {
    pub buffer_id: String,
    pub idx: usize,
    pub count: u64,
    pub byte_size: u64,
    pub total_count: u64,
    pub total_byte_size: u64,
}

impl InternalEvent for BufferEventsSent {
    #[expect(clippy::cast_precision_loss)]
    fn emit(self) {
        counter!(
            CounterName::BufferSentEventsTotal,
            "buffer_id" => self.buffer_id.clone(),
            "stage" => self.idx.to_string()
        )
        .increment(self.count);
        counter!(
            CounterName::BufferSentBytesTotal,
            "buffer_id" => self.buffer_id.clone(),
            "stage" => self.idx.to_string()
        )
        .increment(self.byte_size);
        // DEPRECATED: buffer-bytes-events-metrics
        gauge!(
            GaugeName::BufferEvents,
            "buffer_id" => self.buffer_id.clone(),
            "stage" => self.idx.to_string()
        )
        .set(self.total_count as f64);
        gauge!(
            GaugeName::BufferSizeEvents,
            "buffer_id" => self.buffer_id.clone(),
            "stage" => self.idx.to_string()
        )
        .set(self.total_count as f64);
        gauge!(
            GaugeName::BufferSizeBytes,
            "buffer_id" => self.buffer_id.clone(),
            "stage" => self.idx.to_string()
        )
        .set(self.total_byte_size as f64);
        // DEPRECATED: buffer-bytes-events-metrics
        gauge!(
            GaugeName::BufferByteSize,
            "buffer_id" => self.buffer_id,
            "stage" => self.idx.to_string()
        )
        .set(self.total_byte_size as f64);
    }
}

#[derive(NamedInternalEvent)]
pub struct BufferEventsDropped {
    pub buffer_id: String,
    pub idx: usize,
    pub count: u64,
    pub byte_size: u64,
    pub total_count: u64,
    pub total_byte_size: u64,
    pub intentional: bool,
    pub reason: &'static str,
}

impl InternalEvent for BufferEventsDropped {
    #[expect(clippy::cast_precision_loss)]
    fn emit(self) {
        let intentional_str = if self.intentional { "true" } else { "false" };
        if self.intentional {
            debug!(
                message = "Events dropped.",
                count = %self.count,
                byte_size = %self.byte_size,
                intentional = %intentional_str,
                reason = %self.reason,
                buffer_id = %self.buffer_id,
                stage = %self.idx,
            );
        } else {
            error!(
                message = "Events dropped.",
                count = %self.count,
                byte_size = %self.byte_size,
                intentional = %intentional_str,
                reason = %self.reason,
                buffer_id = %self.buffer_id,
                stage = %self.idx,
            );
        }

        counter!(
            CounterName::BufferDiscardedEventsTotal,
            "buffer_id" => self.buffer_id.clone(),
            "stage" => self.idx.to_string(),
            "intentional" => intentional_str,
        )
        .increment(self.count);
        counter!(
            CounterName::BufferDiscardedBytesTotal,
            "buffer_id" => self.buffer_id.clone(),
            "stage" => self.idx.to_string(),
            "intentional" => intentional_str,
        )
        .increment(self.byte_size);
        // DEPRECATED: buffer-bytes-events-metrics
        gauge!(
            GaugeName::BufferEvents,
            "buffer_id" => self.buffer_id.clone(),
            "stage" => self.idx.to_string()
        )
        .set(self.total_count as f64);
        gauge!(
            GaugeName::BufferSizeEvents,
            "buffer_id" => self.buffer_id.clone(),
            "stage" => self.idx.to_string()
        )
        .set(self.total_count as f64);
        gauge!(
            GaugeName::BufferSizeBytes,
            "buffer_id" => self.buffer_id.clone(),
            "stage" => self.idx.to_string()
        )
        .set(self.total_byte_size as f64);
        // DEPRECATED: buffer-bytes-events-metrics
        gauge!(
            GaugeName::BufferByteSize,
            "buffer_id" => self.buffer_id,
            "stage" => self.idx.to_string()
        )
        .set(self.total_byte_size as f64);
    }
}

#[derive(NamedInternalEvent)]
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
            CounterName::BufferErrorsTotal, "error_code" => self.error_code,
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
        send_duration: Histogram = histogram!(HistogramName::BufferSendDurationSeconds, "stage" => self.stage.to_string()),
    }

    fn emit(&self, duration: Duration) {
        self.send_duration.record(duration);
    }
}
