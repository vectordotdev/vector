use super::prelude::{error_stage, error_type};
use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct AwsCloudwatchLogsSubscriptionParserError {
    pub(crate) error: serde_json::Error,
}

impl InternalEvent for AwsCloudwatchLogsSubscriptionParserError {
    fn emit_logs(&self) {
        error!(
            message = "Event failed to parse as a CloudWatch Logs subscription JSON message.",
            error = ?self.error,
            error_type = error_type::PARSER_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_secs = 10
        )
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::PROCESSING,
        );
        // deprecated
        counter!(
            "processing_errors_total", 1,
            "error_type" => "failed_parse",
        );
    }
}

#[derive(Debug)]
pub struct AwsCloudwatchLogsMessageSizeError {
    pub size: usize,
    pub max_size: usize,
}

impl InternalEvent for AwsCloudwatchLogsMessageSizeError {
    fn emit_logs(&self) {
        error!(
            message = %format!("Encoded event is too long: {} > {}", self.size, self.max_size),
            error_code = "message_too_long",
            error_type = error_type::ENCODER_FAILED,
            stage = error_stage::PROCESSING,
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error_code" => "message_too_long",
            "error_type" => error_type::ENCODER_FAILED,
            "stage" => error_stage::PROCESSING,
        );
        counter!(
            "component_discarded_events_total", 1,
            "error_code" => "message_too_long",
            "error_type" => error_type::ENCODER_FAILED,
            "stage" => error_stage::PROCESSING,
        );
    }
}

#[derive(Debug)]
pub struct AwsCloudwatchLogsEncoderError {
    pub error: std::io::Error,
}

impl InternalEvent for AwsCloudwatchLogsEncoderError {
    fn emit_logs(&self) {
        error!(
            message = "Error when encoding event.",
            error = %self.error,
            error_type = error_type::ENCODER_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_secs = 10,
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error_type" => error_type::ENCODER_FAILED,
            "stage" => error_stage::PROCESSING,
        );
        counter!(
            "component_discarded_events_total", 1,
            "error_type" => error_type::ENCODER_FAILED,
            "stage" => error_stage::PROCESSING,
        );
    }
}
