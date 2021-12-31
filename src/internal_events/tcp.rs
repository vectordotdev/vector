// ## skip check-events ##

use std::net::IpAddr;

use metrics::counter;
use vector_core::internal_event::InternalEvent;

use crate::tls::TlsError;

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
    pub error: TlsError,
}

impl InternalEvent for TcpSocketConnectionError {
    fn emit_logs(&self) {
        match self.error {
            // Specific error that occurs when the other side is only
            // doing SYN/SYN-ACK connections for healthcheck.
            // https://github.com/timberio/vector/issues/7318
            TlsError::Handshake { ref source }
                if source.code() == openssl::ssl::ErrorCode::SYSCALL
                    && source.io_error().is_none() =>
            {
                debug!(message = "Connection error, probably a healthcheck.", error = %self.error, internal_log_rate_secs = 10);
            }
            _ => {
                warn!(message = "Connection error.", error = %self.error, internal_log_rate_secs = 10)
            }
        }
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
        warn!(message = "TCP socket error.", error = %self.error);
    }

    fn emit_metrics(&self) {
        counter!("connection_errors_total", 1, "mode" => "tcp");
    }
}

#[derive(Debug)]
pub struct TcpSendAckError {
    pub error: std::io::Error,
}

impl InternalEvent for TcpSendAckError {
    fn emit_logs(&self) {
        warn!(message = "Error writing acknowledgement, dropping connection.", error = %self.error);
    }

    fn emit_metrics(&self) {
        counter!("connection_send_ack_errors_total", 1, "mode" => "tcp");
    }
}

#[derive(Debug)]
pub struct TcpBytesReceived {
    pub byte_size: usize,
    pub peer_addr: IpAddr,
}

impl InternalEvent for TcpBytesReceived {
    fn emit_logs(&self) {
        trace!(message = "Bytes received.", byte_size = %self.byte_size, peer_addr = %self.peer_addr);
    }

    fn emit_metrics(&self) {
        counter!(
            "component_received_bytes_total", self.byte_size as u64,
            "protocol" => "tcp",
            "peer_addr" => self.peer_addr.to_string()
        );
    }
}
