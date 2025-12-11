use metrics::counter;
use vector_lib::NamedInternalEvent;
use vector_lib::internal_event::{InternalEvent, error_stage, error_type};

#[derive(Debug, NamedInternalEvent)]
pub struct PostgresqlMetricsCollectError<'a> {
    pub error: String,
    pub endpoint: &'a str,
}

impl InternalEvent for PostgresqlMetricsCollectError<'_> {
    fn emit(self) {
        error!(
            message = "PostgreSQL query error.",
            error = %self.error,
            error_type = error_type::REQUEST_FAILED,
            stage = error_stage::RECEIVING,
            endpoint = %self.endpoint,
        );
        counter!(
            "component_errors_total",
            "error_type" => error_type::REQUEST_FAILED,
            "stage" => error_stage::RECEIVING,
        )
        .increment(1);
    }
}
