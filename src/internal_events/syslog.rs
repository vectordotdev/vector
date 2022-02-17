// ## skip check-events ##

use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[cfg(feature = "codecs")]
#[derive(Debug)]
pub struct SyslogUdpReadError {
    pub error: crate::codecs::decoding::Error,
}

#[cfg(feature = "codecs")]
impl InternalEvent for SyslogUdpReadError {
    fn emit_logs(&self) {
        error!(message = "Error reading datagram.", error = ?self.error, internal_log_rate_secs = 10);
    }

    fn emit_metrics(&self) {
        counter!("connection_read_errors_total", 1, "mode" => "udp");
    }
}
#[derive(Debug)]
pub(crate) struct SyslogConvertUtf8Error {
    pub(crate) error: std::str::Utf8Error,
}

impl InternalEvent for SyslogConvertUtf8Error {
    fn emit_logs(&self) {
        error!(message = "Error converting bytes to UTF-8 string.", error = %self.error, internal_log_rate_secs = 10);
    }

    fn emit_metrics(&self) {
        counter!("utf8_convert_errors_total", 1);
    }
}
