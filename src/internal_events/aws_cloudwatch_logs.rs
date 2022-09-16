// ## skip check-dropped-events ##

use metrics::counter;
use vector_common::internal_event::{error_stage, error_type};
use vector_core::internal_event::InternalEvent;

use super::prelude::io_error_code;

#[derive(Debug)]
pub struct AwsCloudwatchLogsMessageSizeError {
    pub size: usize,
    pub max_size: usize,
}

impl InternalEvent for AwsCloudwatchLogsMessageSizeError {
    fn emit(self) {
        error!(
            message = "Encoded event is too long.",
            size = self.size as u64,
            max_size = self.max_size as u64,
            error_code = "message_too_long",
            error_type = error_type::ENCODER_FAILED,
            stage = error_stage::PROCESSING,
        );
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
    pub error: codecs::encoding::Error,
}

impl InternalEvent for AwsCloudwatchLogsEncoderError {
    fn emit(self) {
        let error_code = io_error_code(&std::io::ErrorKind::InvalidData.into());
        error!(
            message = "Error when encoding event.",
            error = %self.error,
            error_type = error_type::ENCODER_FAILED,
            stage = error_stage::PROCESSING,
            error_code = error_code,
            internal_log_rate_secs = 10,
        );
        counter!(
            "component_errors_total", 1,
            "error_type" => error_type::ENCODER_FAILED,
            "error_code" => error_code,
            "stage" => error_stage::PROCESSING,
        );
        counter!(
            "component_discarded_events_total", 1,
            "error_type" => error_type::ENCODER_FAILED,
            "error_code" => error_code,
            "stage" => error_stage::PROCESSING,
        );
    }
}
