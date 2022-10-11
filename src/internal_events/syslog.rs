use metrics::counter;
use vector_core::internal_event::InternalEvent;

use vector_common::internal_event::{error_stage, error_type};

#[derive(Debug)]
pub struct SyslogUdpReadError {
    pub error: codecs::decoding::Error,
}

impl InternalEvent for SyslogUdpReadError {
    fn emit(self) {
        error!(
            message = "Error reading datagram.",
            error = ?self.error,
            error_type = error_type::READER_FAILED,
            stage = error_stage::RECEIVING,
            internal_log_rate_limit = true,
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
