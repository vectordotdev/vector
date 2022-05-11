use metrics::counter;
use vector_core::internal_event::InternalEvent;

use super::prelude::{error_stage, error_type};

#[derive(Debug)]
pub struct EventStoreDbMetricsHttpError {
    pub error: crate::Error,
}

impl InternalEvent for EventStoreDbMetricsHttpError {
    fn emit(self) {
        error!(
            message = "HTTP request processing error.",
            error = ?self.error,
            stage = error_stage::RECEIVING,
            error_type = error_type::REQUEST_FAILED,
        );
        counter!(
            "component_errors_total", 1,
            "stage" => error_stage::RECEIVING,
            "error_type" => error_type::REQUEST_FAILED,
        );
        // deprecated
        counter!("http_request_errors_total", 1);
    }
}

#[derive(Debug)]
pub struct EventStoreDbStatsParsingError {
    pub error: serde_json::Error,
}

impl InternalEvent for EventStoreDbStatsParsingError {
    fn emit(self) {
        error!(
            message = "JSON parsing error.",
            error = ?self.error,
            stage = error_stage::PROCESSING,
            error_type = error_type::PARSER_FAILED,
        );
        counter!(
            "component_errors_total", 1,
            "stage" => error_stage::PROCESSING,
            "error_type" => error_type::PARSER_FAILED,
        );
        // deprecated
        counter!("parse_errors_total", 1);
    }
}
