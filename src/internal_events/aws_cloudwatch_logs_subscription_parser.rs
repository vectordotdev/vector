use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub(crate) struct AwsCloudwatchLogsSubscriptionParserFailedParse {
    pub error: serde_json::Error,
}

impl InternalEvent for AwsCloudwatchLogsSubscriptionParserFailedParse {
    fn emit_logs(&self) {
        warn!(
            message = "Event failed to parse as a CloudWatch Logs subscirption JSON message.",
            error = ?self.error,
            internal_log_rate_secs = 30
        )
    }

    fn emit_metrics(&self) {
        counter!("processing_errors_total", 1,
            "error_type" => "failed_parse",
        );
    }
}
