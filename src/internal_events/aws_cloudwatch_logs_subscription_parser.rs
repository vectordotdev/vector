use super::prelude::error_stage;
use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct AwsCloudwatchLogsSubscriptionParserError {
    pub error: serde_json::Error,
}

impl InternalEvent for AwsCloudwatchLogsSubscriptionParserError {
    fn emit_logs(&self) {
        error!(
            message = "Event failed to parse as a CloudWatch Logs subscription JSON message.",
            error = ?self.error,
            error_type = "parser_failed",
            stage = error_stage::PROCESSING,
            internal_log_rate_secs = 30
        )
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error" => self.error.to_string(),
            "error_type" => "parser_failed",
            "stage" => error_stage::PROCESSING,
        );
        // deprecated
        counter!("processing_errors_total", 1,
            "error_type" => "failed_parse",
        );
    }
}
