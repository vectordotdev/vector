use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub(crate) struct AwsCloudwatchLogsSubscriptionParserEventProcessed;

impl InternalEvent for AwsCloudwatchLogsSubscriptionParserEventProcessed {
    fn emit_logs(&self) {
        trace!(message = "Received one event.");
    }

    fn emit_metrics(&self) {
        counter!("events_processed", 1,
            "component_kind" => "transform",
            "component_type" => "aws_cloudwatch_logs_subscription_parser",
        );
    }
}

#[derive(Debug)]
pub(crate) struct AwsCloudwatchLogsSubscriptionParserFailedParse {
    pub error: serde_json::Error,
}

impl InternalEvent for AwsCloudwatchLogsSubscriptionParserFailedParse {
    fn emit_logs(&self) {
        warn!(
            message = "Event failed to parse as a CloudWatch Logs subscirption JSON message.",
            %self.error,
            rate_limit_secs = 30
        )
    }

    fn emit_metrics(&self) {
        counter!("processing_errors", 1,
            "component_kind" => "transform",
            "component_type" => "aws_cloudwatch_logs_subscription_parser",
            "error_type" => "failed_parse",
        );
    }
}
