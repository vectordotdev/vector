use metrics::counter;
use pulsar::error::ConsumerError;
use vector_core::internal_event::InternalEvent;

use crate::emit;
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
