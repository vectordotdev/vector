use crate::internal_events::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct EventStoreDbMetricsHttpError {
    pub error: crate::Error,
}

impl InternalEvent for EventStoreDbMetricsHttpError {
    fn emit_logs(&self) {
        error!(message = "HTTP request processing error.", error = ?self.error);
    }

    fn emit_metrics(&self) {
        counter!("http_request_errors_total", 1);
    }
}

#[derive(Debug)]
pub struct EventStoreDbStatsParsingError {
    pub error: serde_json::Error,
}

impl InternalEvent for EventStoreDbStatsParsingError {
    fn emit_logs(&self) {
        error!(message = "JSON parsing error.", error = ?self.error);
    }

    fn emit_metrics(&self) {
        counter!("parse_errors_total", 1);
    }
}

pub struct EventStoreDbMetricsReceived {
    pub events: usize,
    pub byte_size: usize,
}

impl InternalEvent for EventStoreDbMetricsReceived {
    fn emit_logs(&self) {
        debug!("Stats scraped.");
    }

    fn emit_metrics(&self) {
        counter!("received_events_total", self.events as u64);
        counter!("processed_bytes_total", self.byte_size as u64);
    }
}
