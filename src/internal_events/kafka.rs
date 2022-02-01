use metrics::{counter, gauge};

use vector_core::{internal_event::InternalEvent, update_counter};

#[derive(Debug)]
pub struct KafkaEventsReceived {
    pub byte_size: usize,
    pub count: usize,
}

impl InternalEvent for KafkaEventsReceived {
    fn emit_logs(&self) {
        trace!(
            message = "Received events.",
            count = %self.count,
            byte_size = %self.byte_size,
        );
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
pub struct KafkaOffsetUpdateError {
    pub error: rdkafka::error::KafkaError,
}

impl InternalEvent for KafkaOffsetUpdateError {
    fn emit_logs(&self) {
        error!(
            message = "Unable to update consumer offset.",
            error = %self.error,
            error_type = "kafka_offset_update",
            stage = "sending",
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error" => self.error.to_string(),
            "error_type" => "kafka_offset_update",
            "stage" => "sending",
        );
        // deprecated
        counter!("consumer_offset_updates_failed_total", 1);
    }
}

#[derive(Debug)]
pub struct KafkaReadError {
    pub error: rdkafka::error::KafkaError,
}

impl InternalEvent for KafkaReadError {
    fn emit_logs(&self) {
        error!(
            message = "Failed to read message.",
            error = %self.error,
            error_type = "kafka_read",
            stage = "receiving",
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error" => self.error.to_string(),
            "error_type" => "kafka_read",
            "stage" => "receiving",
        );
        // deprecated
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

#[derive(Debug)]
pub struct KafkaStatisticsReceived<'a> {
    pub statistics: &'a rdkafka::Statistics,
}

impl InternalEvent for KafkaStatisticsReceived<'_> {
    fn emit_metrics(&self) {
        gauge!("kafka_queue_messages", self.statistics.msg_cnt as f64);
        gauge!(
            "kafka_queue_messages_bytes",
            self.statistics.msg_size as f64
        );
        update_counter!("kafka_requests_total", self.statistics.tx as u64);
        update_counter!(
            "kafka_requests_bytes_total",
            self.statistics.tx_bytes as u64
        );
        update_counter!("kafka_responses_total", self.statistics.rx as u64);
        update_counter!(
            "kafka_responses_bytes_total",
            self.statistics.rx_bytes as u64
        );
        update_counter!(
            "kafka_produced_messages_total",
            self.statistics.txmsgs as u64
        );
        update_counter!(
            "kafka_produced_messages_bytes_total",
            self.statistics.txmsg_bytes as u64
        );
        update_counter!(
            "kafka_consumed_messages_total",
            self.statistics.rxmsgs as u64
        );
        update_counter!(
            "kafka_consumed_messages_bytes_total",
            self.statistics.rxmsg_bytes as u64
        );
    }
}

pub struct KafkaHeaderExtractionFailed<'a> {
    pub header_field: &'a str,
}

impl InternalEvent for KafkaHeaderExtractionFailed<'_> {
    fn emit_logs(&self) {
        error!(
            message = "Failed to extract header. Value should be a map of String -> Bytes.",
            header_field = self.header_field
        );
    }

    fn emit_metrics(&self) {
        counter!("kafka_header_extraction_failures_total", 1);
    }
}
