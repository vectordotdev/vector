#![allow(dead_code)] // TODO requires optional feature compilation

use vector_lib::{
    NamedInternalEvent, counter, gauge,
    internal_event::{CounterName, InternalEvent, error_stage, error_type},
    json_size::JsonSize,
};
use vrl::path::OwnedTargetPath;

#[derive(Debug, NamedInternalEvent)]
pub struct KafkaBytesReceived<'a> {
    pub byte_size: usize,
    pub protocol: &'static str,
    pub topic: &'a str,
    pub partition: i32,
}

impl InternalEvent for KafkaBytesReceived<'_> {
    fn emit(self) {
        trace!(
            message = "Bytes received.",
            byte_size = %self.byte_size,
            protocol = %self.protocol,
            topic = self.topic,
            partition = %self.partition,
        );
        counter!(
            CounterName::ComponentReceivedBytesTotal,
            "protocol" => self.protocol,
            "topic" => self.topic.to_string(),
            "partition" => self.partition.to_string(),
        )
        .increment(self.byte_size as u64);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct KafkaEventsReceived<'a> {
    pub byte_size: JsonSize,
    pub count: usize,
    pub topic: &'a str,
    pub partition: i32,
}

impl InternalEvent for KafkaEventsReceived<'_> {
    fn emit(self) {
        trace!(
            message = "Events received.",
            count = %self.count,
            byte_size = %self.byte_size,
            topic = self.topic,
            partition = %self.partition,
        );
        counter!(
            CounterName::ComponentReceivedEventsTotal,
            "topic" => self.topic.to_string(),
            "partition" => self.partition.to_string(),
        )
        .increment(self.count as u64);
        counter!(
            CounterName::ComponentReceivedEventBytesTotal,
            "topic" => self.topic.to_string(),
            "partition" => self.partition.to_string(),
        )
        .increment(self.byte_size.get() as u64);
    }
}

#[derive(Debug, NamedInternalEvent)]
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
            CounterName::ComponentErrorsTotal,
            "error_code" => "kafka_offset_update",
            "error_type" => error_type::READER_FAILED,
            "stage" => error_stage::SENDING,
        )
        .increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
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
            CounterName::ComponentErrorsTotal,
            "error_code" => "reading_message",
            "error_type" => error_type::READER_FAILED,
            "stage" => error_stage::RECEIVING,
        )
        .increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct KafkaStatisticsReceived<'a> {
    pub statistics: &'a rdkafka::Statistics,
    pub expose_lag_metrics: bool,
}

impl InternalEvent for KafkaStatisticsReceived<'_> {
    fn emit(self) {
        gauge!(CounterName::KafkaQueueMessages).set(self.statistics.msg_cnt as f64);
        gauge!(CounterName::KafkaQueueMessagesBytes).set(self.statistics.msg_size as f64);
        counter!(CounterName::KafkaRequestsTotal).absolute(self.statistics.tx as u64);
        counter!(CounterName::KafkaRequestsBytesTotal).absolute(self.statistics.tx_bytes as u64);
        counter!(CounterName::KafkaResponsesTotal).absolute(self.statistics.rx as u64);
        counter!(CounterName::KafkaResponsesBytesTotal).absolute(self.statistics.rx_bytes as u64);
        counter!(CounterName::KafkaProducedMessagesTotal).absolute(self.statistics.txmsgs as u64);
        counter!(CounterName::KafkaProducedMessagesBytesTotal)
            .absolute(self.statistics.txmsg_bytes as u64);
        counter!(CounterName::KafkaConsumedMessagesTotal).absolute(self.statistics.rxmsgs as u64);
        counter!(CounterName::KafkaConsumedMessagesBytesTotal)
            .absolute(self.statistics.rxmsg_bytes as u64);

        if self.expose_lag_metrics {
            for (topic_id, topic) in &self.statistics.topics {
                for (partition_id, partition) in &topic.partitions {
                    gauge!(
                        CounterName::KafkaConsumerLag,
                        "topic_id" => topic_id.clone(),
                        "partition_id" => partition_id.to_string(),
                    )
                    .set(partition.consumer_lag as f64);
                }
            }
        }
    }
}

#[derive(NamedInternalEvent)]
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
        );
        counter!(
            CounterName::ComponentErrorsTotal,
            "error_code" => "extracting_header",
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::RECEIVING,
        )
        .increment(1);
    }
}
