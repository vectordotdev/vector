// ## skip check-events ##

use std::io::Error;

use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct NatsEventsReceived {
    pub byte_size: usize,
    pub count: usize,
}

impl InternalEvent for NatsEventsReceived {
    fn emit_logs(&self) {
        trace!(
            message = "Received events.",
            self.count,
            internal_log_rate_secs = 10
        );
    }

    fn emit_metrics(&self) {
        counter!("component_received_events_total", self.count as u64);
        counter!("events_in_total", self.count as u64);
        counter!("processed_bytes_total", self.byte_size as u64);
    }
}

#[derive(Debug)]
pub struct NatsEventSendSuccess {
    pub byte_size: usize,
}

impl InternalEvent for NatsEventSendSuccess {
    fn emit_logs(&self) {
        trace!(message = "Processed one event.");
    }

    fn emit_metrics(&self) {
        counter!("processed_bytes_total", self.byte_size as u64);
    }
}

#[derive(Debug)]
pub struct NatsEventSendFail {
    pub error: Error,
}

impl InternalEvent for NatsEventSendFail {
    fn emit_logs(&self) {
        error!(message = "Failed to send message.", error = %self.error);
    }

    fn emit_metrics(&self) {
        counter!("send_errors_total", 1);
    }
}
