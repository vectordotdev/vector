use metrics::{counter, gauge};
use vector_lib::internal_event::InternalEvent;
use vector_lib::{
    internal_event::{error_stage, error_type},
    json_size::JsonSize,
};
use vrl::path::OwnedTargetPath;

#[derive(Debug)]
pub struct KafkaBytesReceived<'a> {
    pub byte_size: usize,
    pub protocol: &'static str,
    pub topic: &'a str,
    pub partition: i32,
}

impl<'a> InternalEvent for KafkaBytesReceived<'a> {
    fn emit(self) {
        trace!(
            message = "Bytes received.",
            byte_size = %self.byte_size,
            protocol = %self.protocol,
            topic = self.topic,
            partition = %self.partition,
        );
        counter!(
            "component_received_bytes_total",
            "protocol" => self.protocol,
            "topic" => self.topic.to_string(),
            "partition" => self.partition.to_string(),
        )
        .increment(self.byte_size as u64);
    }
}

#[derive(Debug)]
pub struct KafkaEventsReceived<'a> {
    pub byte_size: JsonSize,
    pub count: usize,
    pub topic: &'a str,
    pub partition: i32,
}

impl<'a> InternalEvent for KafkaEventsReceived<'a> {
    fn emit(self) {
        trace!(
            message = "Events received.",
            count = %self.count,
            byte_size = %self.byte_size,
            topic = self.topic,
            partition = %self.partition,
        );
        counter!(
            "component_received_events_total",
            "topic" => self.topic.to_string(),
            "partition" => self.partition.to_string(),
        )
        .increment(self.count as u64);
        counter!(
            "component_received_event_bytes_total",
            "topic" => self.topic.to_string(),
            "partition" => self.partition.to_string(),
        )
        .increment(self.byte_size.get() as u64);
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
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total",
            "error_code" => "kafka_offset_update",
            "error_type" => error_type::READER_FAILED,
            "stage" => error_stage::SENDING,
        )
        .increment(1);
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
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total",
            "error_code" => "reading_message",
            "error_type" => error_type::READER_FAILED,
            "stage" => error_stage::RECEIVING,
        )
        .increment(1);
    }
}

#[derive(Debug)]
pub struct KafkaStatisticsReceived<'a> {
    pub statistics: &'a rdkafka::Statistics,
    pub expose_lag_metrics: bool,
}

impl InternalEvent for KafkaStatisticsReceived<'_> {
    fn emit(self) {
        gauge!("kafka_queue_messages").set(self.statistics.msg_cnt as f64);
        gauge!("kafka_queue_messages_bytes").set(self.statistics.msg_size as f64);
        counter!("kafka_requests_total").absolute(self.statistics.tx as u64);
        counter!("kafka_requests_bytes_total").absolute(self.statistics.tx_bytes as u64);
        counter!("kafka_responses_total").absolute(self.statistics.rx as u64);
        counter!("kafka_responses_bytes_total").absolute(self.statistics.rx_bytes as u64);
        counter!("kafka_produced_messages_total").absolute(self.statistics.txmsgs as u64);
        counter!("kafka_produced_messages_bytes_total")
            .absolute(self.statistics.txmsg_bytes as u64);
        counter!("kafka_consumed_messages_total").absolute(self.statistics.rxmsgs as u64);
        counter!("kafka_consumed_messages_bytes_total")
            .absolute(self.statistics.rxmsg_bytes as u64);

        if self.expose_lag_metrics {
            for (topic_id, topic) in &self.statistics.topics {
                for (partition_id, partition) in &topic.partitions {
                    gauge!(
                        "kafka_consumer_lag",
                        "topic_id" => topic_id.clone(),
                        "partition_id" => partition_id.to_string(),
                    )
                    .set(partition.consumer_lag as f64);
                }
            }
        }
    }
}

pub struct KafkaHeaderExtractionError<'a> {
    pub header_field: &'a OwnedTargetPath,
}

impl InternalEvent for KafkaHeaderExtractionError<'_> {
    fn emit(self) {
        error!(
            message = "Failed to extract header. Value should be a map of String -> Bytes.",
            error_code = "extracting_header",
            error_type = error_type::PARSER_FAILED,
            stage = error_stage::RECEIVING,
            header_field = self.header_field.to_string(),
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total",
            "error_code" => "extracting_header",
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::RECEIVING,
        )
        .increment(1);
    }
}
