use super::prelude::error_stage;
use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct EventStoreDbMetricsEventsReceived {
    pub count: usize,
    pub byte_size: usize,
}

impl InternalEvent for EventStoreDbMetricsEventsReceived {
    fn emit_logs(&self) {
        trace!(message = "Events received.", count = %self.count, byte_size = %self.byte_size);
    }

    fn emit_metrics(&self) {
        counter!("component_received_events_total", self.count as u64);
        counter!(
            "component_received_event_bytes_total",
            self.byte_size as u64
        );
        // deprecated
        counter!("events_in_total", self.count as u64);
        counter!("processed_bytes_total", self.byte_size as u64);
    }
}

#[derive(Debug)]
pub struct EventStoreDbMetricsHttpError {
    pub error: crate::Error,
}

impl InternalEvent for EventStoreDbMetricsHttpError {
    fn emit_logs(&self) {
        error!(
            message = "HTTP request processing error.",
            error = ?self.error,
            error_type = "http_error",
            stage = error_stage::RECEIVING,
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "stage" => error_stage::RECEIVING,
            "error" => self.error.to_string(),
            "error_type" => "http_error",
        );
        // deprecated
        counter!("http_request_errors_total", 1);
    }
}

#[derive(Debug)]
pub struct EventStoreDbStatsParsingError {
    pub error: serde_json::Error,
}

impl InternalEvent for EventStoreDbStatsParsingError {
    fn emit_logs(&self) {
        error!(
            message = "JSON parsing error.",
            error = ?self.error,
            error_type = "parse_failed",
            stage = error_stage::PROCESSING,
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "stage" => error_stage::PROCESSING,
            "error" => self.error.to_string(),
            "error_type" => "parse_failed",
        );
        // deprecated
        counter!("parse_errors_total", 1);
    }
}
