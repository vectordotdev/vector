use crate::internal_events::InternalEvent;
use metrics::{counter, histogram};
use std::time::Instant;

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
        histogram!("collect_duration_nanoseconds", self.end - self.start);
    }
}

#[derive(Debug)]
pub struct PostgresqlMetricsCollectFailed<'a> {
    pub error: String,
    pub endpoint: Option<&'a String>,
}

impl<'a> InternalEvent for PostgresqlMetricsCollectFailed<'a> {
    fn emit_logs(&self) {
        let message = "PostgreSQL query error.";
        match self.endpoint {
            Some(endpoint) => error!(message, error = %self.error, %endpoint),
            None => error!(message, error = %self.error),
        }
    }

    fn emit_metrics(&self) {
        counter!("request_errors_total", 1);
    }
}
