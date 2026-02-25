use metrics::counter;
use vector_lib::NamedInternalEvent;
use vector_lib::internal_event::{InternalEvent, error_stage, error_type};

use super::prelude::io_error_code;

#[derive(Debug, NamedInternalEvent)]
pub struct HerokuLogplexRequestReceived<'a> {
    pub msg_count: usize,
    pub frame_id: &'a str,
    pub drain_token: &'a str,
}

impl InternalEvent for HerokuLogplexRequestReceived<'_> {
    fn emit(self) {
        debug!(
            message = "Handling logplex request.",
            msg_count = %self.msg_count,
            frame_id = %self.frame_id,
            drain_token = %self.drain_token
        );
    }
}

#[derive(Debug, NamedInternalEvent)]
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
