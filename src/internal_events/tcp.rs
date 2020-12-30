use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct TcpSocketConnectionEstablished {
    pub peer_addr: Option<std::net::SocketAddr>,
}

impl InternalEvent for TcpSocketConnectionEstablished {
    fn emit_logs(&self) {
        if let Some(peer_addr) = self.peer_addr {
            debug!(message = "Connected.", %peer_addr);
        } else {
            debug!(message = "Connected.", peer_addr = "unknown");
        }
    }

    fn emit_metrics(&self) {
        counter!("connection_established_total", 1, "mode" => "tcp");
    }
}

#[derive(Debug)]
pub struct TcpSocketConnectionFailed<E> {
    pub error: E,
}

impl<E> InternalEvent for TcpSocketConnectionFailed<E>
where
    E: std::error::Error,
{
    fn emit_logs(&self) {
        error!(message = "Unable to connect.", error = %self.error);
    }

    fn emit_metrics(&self) {
        counter!("connection_failed_total", 1, "mode" => "tcp");
    }
}

#[derive(Debug)]
pub struct TcpSocketConnectionShutdown;

impl InternalEvent for TcpSocketConnectionShutdown {
    fn emit_logs(&self) {
        debug!(message = "Received EOF from the server, shutdown.");
    }

    fn emit_metrics(&self) {
        counter!("connection_shutdown_total", 1, "mode" => "tcp");
    }
}

#[derive(Debug)]
pub struct TcpSocketConnectionError {
    pub error: crate::tls::TlsError,
}

impl InternalEvent for TcpSocketConnectionError {
    fn emit_logs(&self) {
        warn!(message = "Connection error.", error = %self.error, internal_log_rate_secs = 10);
    }

    fn emit_metrics(&self) {
        counter!("connection_errors_total", 1, "mode" => "tcp");
    }
}

#[derive(Debug)]
pub struct TcpSocketError {
    pub error: std::io::Error,
}

impl InternalEvent for TcpSocketError {
    fn emit_logs(&self) {
        debug!(message = "TCP socket error.", error = %self.error);
    }

    fn emit_metrics(&self) {
        counter!("connection_errors_total", 1, "mode" => "tcp");
    }
}
