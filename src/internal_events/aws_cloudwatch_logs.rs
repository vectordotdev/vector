use metrics::counter;
use vector_common::internal_event::{error_stage, error_type};
use vector_core::internal_event::InternalEvent;

use crate::{
    emit,
    internal_events::{ComponentEventsDropped, UNINTENTIONAL},
};

#[derive(Debug)]
pub struct AwsCloudwatchLogsMessageSizeError {
    pub size: usize,
    pub max_size: usize,
}

impl InternalEvent for AwsCloudwatchLogsMessageSizeError {
    fn emit(self) {
        let reason = "Encoded event is too long.";
        error!(
            message = reason,
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
        emit!(ComponentEventsDropped::<UNINTENTIONAL> { count: 1, reason });
    }
}
