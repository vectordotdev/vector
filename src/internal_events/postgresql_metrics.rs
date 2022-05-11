use metrics::counter;
use vector_core::internal_event::InternalEvent;

use super::prelude::{error_stage, error_type};

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
        );
        counter!(
            "component_errors_total", 1,
            "error_type" => error_type::REQUEST_FAILED,
            "stage" => error_stage::RECEIVING,
        );
        // deprecated
        counter!("request_errors_total", 1);
    }
}
