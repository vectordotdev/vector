use std::io::Error;

use crate::{
    emit,
    internal_events::{ComponentEventsDropped, UNINTENTIONAL},
};
use metrics::counter;
use vector_core::internal_event::InternalEvent;

use super::prelude::{error_stage, error_type, io_error_code};

#[derive(Debug)]
pub struct NatsEventSendError {
    pub error: Error,
}

impl InternalEvent for NatsEventSendError {
    fn emit(self) {
        let reason = "Failed to send message.";
        error!(
            message = reason,
            error = %self.error,
            error_type = error_type::WRITER_FAILED,
            error_code = io_error_code(&self.error),
            stage = error_stage::SENDING,
            internal_log_rate_secs = 10,
        );
        counter!(
            "component_errors_total", 1,
            "error_type" => error_type::WRITER_FAILED,
            "error_code" => io_error_code(&self.error),
            "stage" => error_stage::SENDING,
        );
        emit!(ComponentEventsDropped::<UNINTENTIONAL> { count: 1, reason });

        // deprecated
        counter!("send_errors_total", 1);
    }
}
