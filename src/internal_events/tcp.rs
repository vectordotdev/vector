use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct TcpConnectionEstablished {
    pub peer_addr: Option<std::net::SocketAddr>,
}

impl InternalEvent for TcpConnectionEstablished {
    fn emit_logs(&self) {
        if let Some(peer_addr) = self.peer_addr {
            debug!(message = "Connected.", %peer_addr);
        } else {
            debug!(message = "Connected.", peer_addr = "unknown");
        }
    }

    fn emit_metrics(&self) {
        counter!("tcp_connections_established", 1);
    }
}

#[derive(Debug)]
pub struct TcpConnectionFailed {
    pub error: crate::tls::TlsError,
}

impl InternalEvent for TcpConnectionFailed {
    fn emit_logs(&self) {
        error!(message = "Unable to connect.", error = %self.error);
    }

    fn emit_metrics(&self) {
        counter!("tcp_connections_failed", 1);
    }
}

#[derive(Debug)]
pub struct TcpConnectionDisconnected {
    pub error: std::io::Error,
}

impl InternalEvent for TcpConnectionDisconnected {
    fn emit_logs(&self) {
        error!(message = "Connection disconnected.", error = %self.error);
    }

    fn emit_metrics(&self) {
        counter!("tcp_connections_disconnected", 1);
    }
}

#[derive(Debug)]
pub struct TcpConnectionShutdown {}

impl InternalEvent for TcpConnectionShutdown {
    fn emit_logs(&self) {
        debug!(message = "Received EOF from the server; reconnecting.");
    }

    fn emit_metrics(&self) {
        counter!("tcp_connection_shutdown", 1, "mode" => "tcp");
    }
}

#[derive(Debug)]
pub struct TcpConnectionError<T> {
    pub error: T,
}

impl<T: std::fmt::Debug + std::fmt::Display> InternalEvent for TcpConnectionError<T> {
    fn emit_logs(&self) {
        warn!(message = "Connection error.", error = %self.error, rate_limit_secs = 10);
    }

    fn emit_metrics(&self) {
        counter!("tcp_connection_errors", 1);
    }
}

#[derive(Debug)]
pub struct TcpFlushError {
    pub error: std::io::Error,
}

impl InternalEvent for TcpFlushError {
    fn emit_logs(&self) {
        error!(message = "Unable to flush connection.", error = %self.error);
    }

    fn emit_metrics(&self) {
        counter!("tcp_flush_errors", 1);
    }
}

#[derive(Debug)]
pub struct TcpEventSent {
    pub byte_size: usize,
}

impl InternalEvent for TcpEventSent {
    fn emit_logs(&self) {
        trace!(message = "Sending event.", byte_size = %self.byte_size);
    }

    fn emit_metrics(&self) {
        counter!("events_processed", 1);
        counter!("bytes_processed", self.byte_size as u64);
    }
}
