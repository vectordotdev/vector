use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct TcpConnectionEstablished {
    pub peer_addr: Option<std::net::SocketAddr>,
}

impl InternalEvent for TcpConnectionEstablished {
    fn emit_logs(&self) {
        if let Some(peer_addr) = self.peer_addr {
            debug!(message = "connected", %peer_addr);
        } else {
            debug!(message = "connected", peer_addr = "unknown");
        }
    }

    fn emit_metrics(&self) {
        counter!("tcp_connections_established", 1,
            "component_kind" => "sink",
        );
    }
}

#[derive(Debug)]
pub struct TcpConnectionFailed {
    pub error: crate::tls::TlsError,
}

impl InternalEvent for TcpConnectionFailed {
    fn emit_logs(&self) {
        error!(message = "unable to connect.", error = %self.error);
    }

    fn emit_metrics(&self) {
        counter!("tcp_connections_failed", 1,
            "component_kind" => "sink",
        );
    }
}

#[derive(Debug)]
pub struct TcpConnectionDisconnected {
    pub error: std::io::Error,
}

impl InternalEvent for TcpConnectionDisconnected {
    fn emit_logs(&self) {
        error!(message = "connection disconnected.", error = %self.error);
    }

    fn emit_metrics(&self) {
        counter!("tcp_connections_disconnected", 1,
            "component_kind" => "sink",
        );
    }
}

#[derive(Debug)]
pub struct TcpConnectionError {
    pub error: std::io::Error,
}

impl InternalEvent for TcpConnectionError {
    fn emit_logs(&self) {
        warn!(message = "connection error.", error = %self.error);
    }

    fn emit_metrics(&self) {
        counter!("tcp_connection_errors", 1,
            "component_kind" => "source",
        );
    }
}

#[derive(Debug)]
pub struct TcpFlushError {
    pub error: std::io::Error,
}

impl InternalEvent for TcpFlushError {
    fn emit_logs(&self) {
        error!(message = "unable to flush connection.", error = %self.error);
    }

    fn emit_metrics(&self) {
        counter!("tcp_flush_errors", 1,
            "component_kind" => "sink",
        );
    }
}

#[derive(Debug)]
pub struct TcpEventSent {
    pub byte_size: usize,
}

impl InternalEvent for TcpEventSent {
    fn emit_logs(&self) {
        debug!(message = "sending event.", byte_size = %self.byte_size);
    }

    fn emit_metrics(&self) {
        counter!("tcp_events_sent", 1,
            "component_kind" => "sink",
        );
        counter!("tcp_bytes_sent", self.byte_size as u64,
            "component_kind" => "sink",
        );
    }
}

#[derive(Debug)]
pub struct TcpEventReceived {
    pub byte_size: usize,
}

impl InternalEvent for TcpEventReceived {
    fn emit_logs(&self) {
        debug!(message = "sending event.", byte_size = %self.byte_size);
    }

    fn emit_metrics(&self) {
        counter!("events_received", 1,
            "component_kind" => "source",
            "component_type" => "socket",
            "mode" => "tcp",
        );
        counter!("bytes_received", self.byte_size as u64,
            "component_kind" => "source",
            "component_type" => "socket",
            "mode" => "tcp",
        );
    }
}
