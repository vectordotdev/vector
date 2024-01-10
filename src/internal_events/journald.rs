use metrics::counter;
use vector_lib::codecs::decoding::BoxedFramingError;
use vector_lib::internal_event::InternalEvent;
use vector_lib::internal_event::{error_stage, error_type};

#[derive(Debug)]
pub struct JournaldInvalidRecordError {
    pub error: serde_json::Error,
    pub text: String,
}

impl InternalEvent for JournaldInvalidRecordError {
    fn emit(self) {
        error!(
            message = "Invalid record from journald, discarding.",
            error = ?self.error,
            text = %self.text,
            error_type = error_type::PARSER_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total",
            "stage" => error_stage::PROCESSING,
            "error_type" => error_type::PARSER_FAILED,
        )
        .increment(1);
    }
}

#[derive(Debug)]
pub struct JournaldStartJournalctlError {
    pub error: crate::Error,
}

impl InternalEvent for JournaldStartJournalctlError {
    fn emit(self) {
        error!(
            message = "Error starting journalctl process.",
            error = %self.error,
            error_type = error_type::COMMAND_FAILED,
            stage = error_stage::RECEIVING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total",
            "stage" => error_stage::RECEIVING,
            "error_type" => error_type::COMMAND_FAILED,
        )
        .increment(1);
    }
}

#[derive(Debug)]
pub struct JournaldReadError {
    pub error: BoxedFramingError,
}

impl InternalEvent for JournaldReadError {
    fn emit(self) {
        error!(
            message = "Could not read from journald.",
            error = %self.error,
            error_type = error_type::READER_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total",
            "stage" => error_stage::PROCESSING,
            "error_type" => error_type::READER_FAILED,
        )
        .increment(1);
    }
}

#[derive(Debug)]
pub struct JournaldCheckpointSetError {
    pub error: std::io::Error,
    pub filename: String,
}

impl InternalEvent for JournaldCheckpointSetError {
    fn emit(self) {
        error!(
            message = "Could not set journald checkpoint.",
            filename = ?self.filename,
            error = %self.error,
            error_type = error_type::IO_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total",
            "stage" => error_stage::PROCESSING,
            "error_type" => error_type::IO_FAILED,
        )
        .increment(1);
    }
}

#[derive(Debug)]
pub struct JournaldCheckpointFileOpenError {
    pub error: std::io::Error,
    pub path: String,
}

impl InternalEvent for JournaldCheckpointFileOpenError {
    fn emit(self) {
        error!(
            message = "Unable to open checkpoint file.",
            path = ?self.path,
            error = %self.error,
            error_type = error_type::IO_FAILED,
            stage = error_stage::RECEIVING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total",
            "stage" => error_stage::RECEIVING,
            "error_type" => error_type::IO_FAILED,
        )
        .increment(1);
    }
}
