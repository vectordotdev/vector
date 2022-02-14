use std::time::Instant;

use metrics::{counter, histogram};
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct PostgresqlMetricsCollectCompleted {
    pub start: Instant,
    pub end: Instant,
}

impl InternalEvent for PostgresqlMetricsCollectCompleted {
    fn emit_logs(&self) {
        debug!(message = "Collection completed.");
    }

    fn emit_metrics(&self) {
        counter!("collect_completed_total", 1);
        histogram!("collect_duration_seconds", self.end - self.start);
    }
}

#[derive(Debug)]
pub struct PostgresqlMetricsCollectError<'a> {
    pub error: String,
    pub endpoint: Option<&'a String>,
}

impl<'a> InternalEvent for PostgresqlMetricsCollectError<'a> {
    fn emit_logs(&self) {
        let message = "PostgreSQL query error.";
        match self.endpoint {
            Some(endpoint) => error!(
                message,
                error = %self.error,
                error_type = "request_error",
                stage = "receiving",
                endpoint = %endpoint,
            ),
            None => error!(
                message,
                error = %self.error,
                error_type = "request_error",
                stage = "receiving",
            ),
        }
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error" => self.error.to_string(),
            "error_type" => "request_error",
            "stage" => "receiving",
        );
        // deprecated
        counter!("request_errors_total", 1);
    }
}
