use super::InternalEvent;
use metrics::counter;
use std::io::Error;

#[derive(Debug)]
pub struct NatsEventProcessed;

impl InternalEvent for NatsEventProcessed {
    fn emit_logs(&self) {
        trace!(message = "Processed one event.");
    }

    fn emit_metrics(&self) {
        counter!(
            "events_processed", 1,
            "component_kind" => "sink",
            "component_type" => "nats",
        );
    }
}

#[derive(Debug)]
pub struct NatsEventSendSuccess {
    pub byte_size: usize,
}

impl InternalEvent for NatsEventSendSuccess {
    fn emit_metrics(&self) {
        counter!(
            "bytes_processed", self.byte_size as u64,
            "component_kind" => "sink",
            "component_type" => "nats",
        );
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
}
