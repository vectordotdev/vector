use metrics::counter;
use vector_lib::internal_event::InternalEvent;
use vector_lib::internal_event::{error_stage, error_type};

#[derive(Debug)]
pub struct FileDescriptorReadError<E> {
    pub error: E,
}

impl<E> InternalEvent for FileDescriptorReadError<E>
where
    E: std::fmt::Display,
{
    fn emit(self) {
        error!(
            message = "Error reading from file descriptor.",
            error = %self.error,
            error_type = error_type::CONNECTION_FAILED,
            stage = error_stage::RECEIVING,
            internal_log_rate_limit = true
        );
        counter!(
            "component_errors_total", 1,
            "error_type" => error_type::CONNECTION_FAILED,
            "stage" => error_stage::RECEIVING,
        );
    }
}
