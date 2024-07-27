use metrics::counter;
use vector_lib::internal_event::InternalEvent;
use vector_lib::internal_event::{error_stage, error_type};

use super::prelude::io_error_code;

#[derive(Debug)]
pub struct HerokuLogplexRequestReceived<'a> {
    pub msg_count: usize,
    pub frame_id: &'a str,
    pub drain_token: &'a str,
}

impl<'a> InternalEvent for HerokuLogplexRequestReceived<'a> {
    fn emit(self) {
        debug!(
            message = "Handling logplex request.",
            msg_count = %self.msg_count,
            frame_id = %self.frame_id,
            drain_token = %self.drain_token,
            internal_log_rate_limit = true
        );
    }
}

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
            "component_errors_total",
            "error_type" => error_type::READER_FAILED,
            "error_code" => io_error_code(&self.error),
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);
    }
}
