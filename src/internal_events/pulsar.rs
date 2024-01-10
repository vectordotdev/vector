use metrics::counter;
use vector_lib::internal_event::InternalEvent;
use vector_lib::internal_event::{error_stage, error_type, ComponentEventsDropped, UNINTENTIONAL};

#[derive(Debug)]
pub struct PulsarSendingError {
    pub count: usize,
    pub error: vector_lib::Error,
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
