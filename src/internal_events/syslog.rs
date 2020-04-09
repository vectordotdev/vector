use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct SyslogEventReceived {
    pub byte_size: usize,
}

impl InternalEvent for SyslogEventReceived {
    fn emit_logs(&self) {
        trace!(message = "received line.", byte_size = %self.byte_size);
    }

    fn emit_metrics(&self) {
        counter!("events_processed", 1,
            "component_kind" => "source",
            "component_type" => "syslog",
        );
        counter!("bytes_processed", self.byte_size as u64,
            "component_kind" => "source",
            "component_type" => "syslog",
        );
    }
}

#[derive(Debug)]
pub struct SyslogUdpReadError {
    pub error: std::io::Error,
}

impl InternalEvent for SyslogUdpReadError {
    fn emit_logs(&self) {
        error!(message = "error reading datagram.", error = %self.error);
    }

    fn emit_metrics(&self) {
        counter!("syslog_udp_read_errors", 1,
            "component_kind" => "source",
            "component_type" => "syslog",
            "mode" => "udp",
        );
    }
}
