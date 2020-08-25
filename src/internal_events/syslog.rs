use super::InternalEvent;
use metrics::counter;

define_events_processed_bytes!(SyslogEventReceived, "source", "syslog");

#[derive(Debug)]
pub struct SyslogUdpReadError {
    pub error: std::io::Error,
}

impl InternalEvent for SyslogUdpReadError {
    fn emit_logs(&self) {
        error!(message = "error reading datagram.", error = %self.error, rate_limit_secs = 10);
    }

    fn emit_metrics(&self) {
        counter!("udp_read_errors", 1,
            "component_kind" => "source",
            "component_type" => "syslog",
            "mode" => "udp",
        );
    }
}

#[derive(Debug)]
pub struct SyslogUdpUtf8Error {
    pub error: std::str::Utf8Error,
}

impl InternalEvent for SyslogUdpUtf8Error {
    fn emit_logs(&self) {
        error!(message = "error converting bytes to UTF8 string in UDP mode.", error = %self.error, rate_limit_secs = 10);
    }

    fn emit_metrics(&self) {
        counter!("udp_utf8_convert_errors", 1,
            "component_kind" => "source",
            "component_type" => "syslog",
            "mode" => "udp",
        );
    }
}
