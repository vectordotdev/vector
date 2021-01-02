use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct KafkaEventReceived {
    pub byte_size: usize,
}

impl InternalEvent for KafkaEventReceived {
    fn emit_logs(&self) {
        trace!(message = "Received one event.", internal_log_rate_secs = 10);
    }

    fn emit_metrics(&self) {
        counter!("processed_events_total", 1);
        counter!("processed_bytes_total", self.byte_size as u64);
    }
}

#[derive(Debug)]
pub struct KafkaOffsetUpdateFailed {
    pub error: rdkafka::error::KafkaError,
}

impl InternalEvent for KafkaOffsetUpdateFailed {
    fn emit_logs(&self) {
        error!(message = "Unable to update consumer offset.", error = ?self.error);
    }

    fn emit_metrics(&self) {
        counter!("consumer_offset_updates_failed_total", 1);
    }
}

#[derive(Debug)]
pub struct KafkaEventFailed {
    pub error: rdkafka::error::KafkaError,
}

impl InternalEvent for KafkaEventFailed {
    fn emit_logs(&self) {
        error!(message = "Failed to read message.", error = ?self.error);
    }

    fn emit_metrics(&self) {
        counter!("events_failed_total", 1);
    }
}

#[derive(Debug)]
pub struct KafkaKeyExtractionFailed<'a> {
    pub key_field: &'a str,
}

impl InternalEvent for KafkaKeyExtractionFailed<'_> {
    fn emit_logs(&self) {
        error!(message = "Failed to extract key.", key_field = %self.key_field);
    }
}
