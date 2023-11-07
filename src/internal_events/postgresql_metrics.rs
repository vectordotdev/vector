use metrics::counter;
use vector_lib::internal_event::InternalEvent;
use vector_lib::internal_event::{error_stage, error_type};

#[derive(Debug)]
pub struct PostgresqlMetricsCollectError<'a> {
    pub error: String,
    pub endpoint: &'a str,
}

impl<'a> InternalEvent for PostgresqlMetricsCollectError<'a> {
    fn emit(self) {
        error!(
            message = "PostgreSQL query error.",
            error = %self.error,
            error_type = error_type::REQUEST_FAILED,
            stage = error_stage::RECEIVING,
            endpoint = %self.endpoint,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total", 1,
            "error_type" => error_type::REQUEST_FAILED,
            "stage" => error_stage::RECEIVING,
        );
    }
}
