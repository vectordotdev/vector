use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct EventStoreDbMetricsHttpError {
    pub error: crate::Error,
}

impl InternalEvent for EventStoreDbMetricsHttpError {
    fn emit_logs(&self) {
        error!(
            message = "HTTP request processing error.",
            error = ?self.error,
            error_type = "http_error",
            stage = "receiving",
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "stage" => "receiving",
            "error" => self.error.to_string(),
            "error_type" => "http_error",
        );
        // deprecated
        counter!("http_request_errors_total", 1);
    }
}

#[derive(Debug)]
pub struct EventStoreDbStatsParsingError {
    pub error: serde_json::Error,
}

impl InternalEvent for EventStoreDbStatsParsingError {
    fn emit_logs(&self) {
        error!(
            message = "JSON parsing error.",
            error = ?self.error,
            error_type = "parse_failed",
            stage = "processing",
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "stage" => "processing",
            "error" => self.error.to_string(),
            "error_type" => "parse_failed",
        );
        // deprecated
        counter!("parse_errors_total", 1);
    }
}
