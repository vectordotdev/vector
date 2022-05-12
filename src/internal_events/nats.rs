use std::io::Error;

use metrics::counter;
use vector_core::internal_event::InternalEvent;

use super::prelude::{error_stage, error_type, io_error_code};

#[derive(Debug)]
pub struct NatsEventSendSuccess {
    pub(crate) byte_size: usize,
}

impl InternalEvent for NatsEventSendSuccess {
    fn emit(self) {
        trace!(message = "Processed one event.");
        counter!("component_sent_bytes_total", self.byte_size as u64);
    }
}

#[derive(Debug)]
pub struct NatsEventSendError {
    pub error: Error,
}

impl InternalEvent for NatsEventSendError {
    fn emit(self) {
        error!(
            message = "Failed to send message.",
            error = %self.error,
            error_type = error_type::WRITER_FAILED,
            error_code = io_error_code(&self.error),
            stage = error_stage::SENDING,
        );
        counter!(
            "component_errors_total", 1,
            "error_type" => error_type::WRITER_FAILED,
            "error_code" => io_error_code(&self.error),
            "stage" => error_stage::SENDING,
        );
        // deprecated
        counter!("send_errors_total", 1);
    }
}
