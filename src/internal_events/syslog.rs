// ## skip check-events ##

use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct SyslogUdpReadError {
    pub error: codecs::decoding::Error,
}

impl InternalEvent for SyslogUdpReadError {
    fn emit(self) {
        error!(message = "Error reading datagram.", error = ?self.error, internal_log_rate_secs = 10);
        counter!("connection_read_errors_total", 1, "mode" => "udp");
    }
}
