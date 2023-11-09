use metrics::counter;
use vector_lib::internal_event::InternalEvent;
use vector_lib::internal_event::{error_stage, error_type};

#[derive(Debug)]
pub struct RedisReceiveEventError {
    error: redis::RedisError,
    error_code: String,
}

impl From<redis::RedisError> for RedisReceiveEventError {
    fn from(error: redis::RedisError) -> Self {
        let error_code = error.code().unwrap_or("UNKNOWN").to_string();
        Self { error, error_code }
    }
}

impl InternalEvent for RedisReceiveEventError {
    fn emit(self) {
        error!(
            message = "Failed to read message.",
            error = %self.error,
            error_code = %self.error_code,
            error_type = error_type::READER_FAILED,
            stage = error_stage::SENDING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total", 1,
            "error_code" => self.error_code,
            "error_type" => error_type::READER_FAILED,
            "stage" => error_stage::RECEIVING,
        );
    }
}
