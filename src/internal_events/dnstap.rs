use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct DnstapEventReceived {
    pub byte_size: usize,
}

impl InternalEvent for DnstapEventReceived {
    fn emit_logs(&self) {
        trace!(message = "Received line.", byte_size = %self.byte_size);
    }

    fn emit_metrics(&self) {
        counter!("processed_events_total", 1);
        counter!("component_received_events_total", 1);
        counter!("events_in_total", 1);
        counter!("processed_bytes_total", self.byte_size as u64);
    }
}

#[derive(Debug)]
pub struct DnstapParseDataError<'a> {
    pub error: &'a str,
}

impl<'a> InternalEvent for DnstapParseDataError<'a> {
    fn emit_logs(&self) {
        error!(
            target = "dnstap event",
            message = "Error occurred while parsing dnstap data.",
            error = ?self.error,
            internal_log_rate_secs = 10);
    }

    fn emit_metrics(&self) {
        counter!("parse_errors_total", 1);
    }
}
