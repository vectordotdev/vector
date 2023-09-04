use metrics::counter;
use pulsar::error::ConsumerError;
use vector_core::internal_event::InternalEvent;

use crate::emit;
use vector_common::json_size::JsonSize;
use vector_common::internal_event::{
    error_stage, error_type, ComponentEventsDropped, UNINTENTIONAL,
};

#[derive(Debug)]
pub struct PulsarSendingError {
    pub count: usize,
    pub error: vector_common::Error,
}

impl InternalEvent for PulsarSendingError {
    fn emit(self) {
        let reason = "A Pulsar sink generated an error.";
        error!(
            message = reason,
            error = %self.error,
            error_type = error_type::REQUEST_FAILED,
            stage = error_stage::SENDING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total", 1,
            "error_type" => error_type::REQUEST_FAILED,
            "stage" => error_stage::SENDING,
        );
        emit!(ComponentEventsDropped::<UNINTENTIONAL> {
            count: self.count,
            reason,
        });
    }
}

pub struct PulsarPropertyExtractionError<F: std::fmt::Display> {
    pub property_field: F,
}

impl<F: std::fmt::Display> InternalEvent for PulsarPropertyExtractionError<F> {
    fn emit(self) {
        error!(
            message = "Failed to extract properties. Value should be a map of String -> Bytes.",
            error_code = "extracting_property",
            error_type = error_type::PARSER_FAILED,
            stage = error_stage::PROCESSING,
            property_field = %self.property_field,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total", 1,
            "error_code" => "extracting_property",
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::PROCESSING,
        );
    }
}

#[derive(Debug)]
pub struct PulsarReadError {
    pub error: pulsar::Error,
}

impl InternalEvent for PulsarReadError {
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
pub struct PulsarAcknowledgmentError {
    pub error: ConsumerError,
}

impl InternalEvent for PulsarAcknowledgmentError {
    fn emit(self) {
        error!(
            message = "Failed to acknowledge message.",
            error = %self.error,
            error_code = "acknowledge_message",
            error_type = error_type::ACKNOWLEDGMENT_FAILED,
            stage = error_stage::RECEIVING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total", 1,
            "error_code" => "acknowledge_message",
            "error_type" => error_type::ACKNOWLEDGMENT_FAILED,
            "stage" => error_stage::RECEIVING,
        );
        // deprecated
        counter!("events_failed_total", 1);
    }
}

#[derive(Debug)]
pub struct PulsarNegativeAcknowledgmentError {
    pub error: ConsumerError,
}

impl InternalEvent for PulsarNegativeAcknowledgmentError {
    fn emit(self) {
        error!(
            message = "Failed to negatively acknowledge message.",
            error = %self.error,
            error_code = "negative_acknowledge_message",
            error_type = error_type::ACKNOWLEDGMENT_FAILED,
            stage = error_stage::RECEIVING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total", 1,
            "error_code" => "negative_acknowledge_message",
            "error_type" => error_type::ACKNOWLEDGMENT_FAILED,
            "stage" => error_stage::RECEIVING,
        );
        // deprecated
        counter!("events_failed_total", 1);
    }
}

#[derive(Debug)]
pub struct PulsarEventsReceived<'a> {
    pub byte_size: JsonSize,
    pub count: usize,
    pub topic: &'a str,
}

impl<'a> InternalEvent for PulsarEventsReceived<'a> {
    fn emit(self) {
        trace!(
            message = "Events received.",
            count = %self.count,
            byte_size = %self.byte_size,
            topic = self.topic,
        );
        counter!("component_received_events_total", self.count as u64, "topic" => self.topic.to_string());
        counter!(
            "component_received_event_bytes_total",
            self.byte_size.get() as u64,
            "topic" => self.topic.to_string(),
        );
    }
}
