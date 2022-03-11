use super::prelude::{error_stage, error_type};
use metrics::{counter, gauge};

use vector_core::{internal_event::InternalEvent, update_counter};

#[derive(Debug)]
pub struct KafkaEventsReceived {
    pub byte_size: usize,
    pub count: usize,
}

impl InternalEvent for KafkaEventsReceived {
    fn emit(self) {
        trace!(
            message = "Events received.",
            count = %self.count,
            byte_size = %self.byte_size,
        );
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
    fn emit(self) {
        error!(
            message = "Unable to update consumer offset.",
            error = %self.error,
            error_code = "kafka_offset_update",
            error_type = error_type::READER_FAILED,
            stage = error_stage::SENDING,
        );
        counter!(
            "component_errors_total", 1,
            "error_code" => "kafka_offset_update",
            "error_type" => error_type::READER_FAILED,
            "stage" => error_stage::SENDING,
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
    fn emit(self) {
        error!(
            message = "Failed to read message.",
            error = %self.error,
            error_code = "reading_message",
            error_type = error_type::READER_FAILED,
            stage = error_stage::RECEIVING,
        );
        counter!(
            "component_errors_total", 1,
            "error_code" => "reading_message",
            "error_type" => error_type::READER_FAILED,
            "stage" => error_stage::RECEIVING,
        );
        // deprecated
        counter!("events_failed_total", 1);
    }
}

#[derive(Debug)]
pub struct KafkaStatisticsReceived<'a> {
    pub statistics: &'a rdkafka::Statistics,
}

impl InternalEvent for KafkaStatisticsReceived<'_> {
    fn emit(self) {
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

pub struct KafkaHeaderExtractionError<'a> {
    pub header_field: &'a str,
}

impl InternalEvent for KafkaHeaderExtractionError<'_> {
    fn emit(self) {
        error!(
            message = "Failed to extract header. Value should be a map of String -> Bytes.",
            error_code = "extracing_header",
            error_type = error_type::PARSER_FAILED,
            stage = error_stage::RECEIVING,
            header_field = self.header_field,
        );
        counter!(
            "component_errors_total", 1,
            "error_code" => "extracing_field",
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::RECEIVING,
        );
        // deprecated
        counter!("kafka_header_extraction_failures_total", 1);
    }
}
