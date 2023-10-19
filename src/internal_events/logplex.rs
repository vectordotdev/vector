use metrics::counter;
use vector_common::internal_event::{error_stage, error_type};
use vector_core::internal_event::InternalEvent;

use super::prelude::io_error_code;

#[derive(Debug)]
pub struct HerokuLogplexRequestReadError {
    pub error: std::io::Error,
}

impl InternalEvent for HerokuLogplexRequestReadError {
    fn emit(self) {
        error!(
            message = "Error reading request body.",
            error = ?self.error,
            error_type = error_type::READER_FAILED,
            error_code = io_error_code(&self.error),
            stage = error_stage::PROCESSING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total", 1,
            "error_type" => error_type::READER_FAILED,
            "error_code" => io_error_code(&self.error),
            "stage" => error_stage::PROCESSING,
        );
    }
}
