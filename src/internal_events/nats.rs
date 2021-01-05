use super::InternalEvent;
use metrics::counter;
use std::io::Error;

#[derive(Debug)]
pub struct NatsEventSendSuccess {
    pub byte_size: usize,
}

impl InternalEvent for NatsEventSendSuccess {
    fn emit_logs(&self) {
        trace!(message = "Processed one event.");
    }

    fn emit_metrics(&self) {
        counter!("processed_events_total", 1);
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

#[derive(Debug)]
pub struct NatsEventMissingKeys<'a> {
    pub keys: &'a [String],
}

impl<'a> InternalEvent for NatsEventMissingKeys<'a> {
    fn emit_logs(&self) {
        warn!(
            message = "Keys do not exist on the event; dropping event.",
            missing_keys = ?self.keys,
            internal_log_rate_secs = 30,
        )
    }

    fn emit_metrics(&self) {
        counter!("missing_keys_total", 1);
    }
}
