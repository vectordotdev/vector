use metrics::counter;
use vector_lib::internal_event::InternalEvent;
use vector_lib::internal_event::{error_stage, error_type};

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
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total",
            "stage" => error_stage::RECEIVING,
            "error_type" => error_type::REQUEST_FAILED,
        )
        .increment(1);
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
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total",
            "stage" => error_stage::PROCESSING,
            "error_type" => error_type::PARSER_FAILED,
        )
        .increment(1);
    }
}
