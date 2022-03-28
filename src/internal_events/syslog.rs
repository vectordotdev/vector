use super::prelude::{error_stage, error_type};
use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[cfg(feature = "codecs")]
#[derive(Debug)]
pub struct SyslogUdpReadError {
    pub error: crate::codecs::decoding::Error,
}

#[cfg(feature = "codecs")]
impl InternalEvent for SyslogUdpReadError {
    fn emit(self) {
        error!(
            message = "Error reading datagram.",
            error = ?self.error,
            internal_log_rate_secs = 10,
            error_type = error_type::READER_FAILED,
            stage = error_stage::RECEIVING,
        );
        counter!(
            "component_errors_total", 1,
            "error_type" => error_type::READER_FAILED,
            "stage" => error_stage::RECEIVING,
        );
        // deprecated
        counter!("connection_read_errors_total", 1, "mode" => "udp");
    }
}
#[derive(Debug)]
pub(crate) struct SyslogConvertUtf8Error {
    pub(crate) error: std::str::Utf8Error,
}

impl InternalEvent for SyslogConvertUtf8Error {
    fn emit(self) {
        error!(
            message = "Error converting bytes to UTF-8 string.",
            error = %self.error,
            internal_log_rate_secs = 10,
            error_type = error_type::PARSER_FAILED,
            stage = error_stage::RECEIVING,
        );
        counter!(
            "component_errors_total", 1,
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::RECEIVING,
        );
        // deprecated
        counter!("utf8_convert_errors_total", 1);
    }
}
