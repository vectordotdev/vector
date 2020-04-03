use super::InternalEvent;
use metrics::counter;

pub struct UdpEventReceived {
    pub byte_size: usize,
}

impl InternalEvent for UdpEventReceived {
    fn emit_logs(&self) {
        trace!(message = "Received one event.");
    }

    fn emit_metrics(&self) {
        counter!("events_received", 1,
            "component_kind" => "source",
            "component_kind" => "socket",
            "mode" => "udp",
        );
        counter!("bytes_received", self.byte_size as u64,
            "component_kind" => "source",
            "component_kind" => "socket",
            "mode" => "udp",
        );
    }
}

pub struct UdpSocketError {
    pub error: std::io::Error,
}

impl InternalEvent for UdpSocketError {
    fn emit_logs(&self) {
        error!(message = "error reading datagram.", error = %self.error);
    }

    fn emit_metrics(&self) {
        counter!("socket_errors", 1,
            "component_kind" => "source",
            "component_kind" => "socket",
            "mode" => "udp",
        );
    }
}
