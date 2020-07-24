use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct KafkaEventReceived {
    pub byte_size: usize,
}

impl InternalEvent for KafkaEventReceived {
    fn emit_logs(&self) {
        trace!(message = "received one event.", rate_limit_secs = 10);
    }

    fn emit_metrics(&self) {
        counter!(
            "events_processed", 1,
            "component_kind" => "source",
            "component_type" => "kafka",
        );
        counter!(
            "bytes_processed", self.byte_size as u64,
            "component_kind" => "source",
            "component_type" => "kafka",
        );
    }
}

#[derive(Debug)]
pub struct KafkaOffsetUpdateFailed {
    pub error: rdkafka::error::KafkaError,
}

impl InternalEvent for KafkaOffsetUpdateFailed {
    fn emit_logs(&self) {
        error!(message = "unable to update consumer offset.", error = %self.error);
    }

    fn emit_metrics(&self) {
        counter!(
            "consumer_offset_updates_failed", 1,
            "component_kind" => "source",
            "component_type" => "kafka",
        );
    }
}

#[derive(Debug)]
pub struct KafkaEventFailed {
    pub error: rdkafka::error::KafkaError,
}

impl InternalEvent for KafkaEventFailed {
    fn emit_logs(&self) {
        error!(message = "failed to read message.", error = %self.error);
    }

    fn emit_metrics(&self) {
        counter!(
            "events_failed", 1,
            "component_kind" => "source",
            "component_type" => "kafka",
        );
    }
}

#[derive(Debug)]
pub struct KafkaPayloadExtractionFailed;

impl InternalEvent for KafkaPayloadExtractionFailed {
    fn emit_logs(&self) {
        error!(message = "failed to extract payload.");
    }
}

#[derive(Debug)]
pub struct KafkaKeyExtractionFailed<'a> {
    pub key_field: &'a str,
}

impl InternalEvent for KafkaKeyExtractionFailed<'_> {
    fn emit_logs(&self) {
        error!(message = "failed to extract key.", key_field = %self.key_field);
    }
}
