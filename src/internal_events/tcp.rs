use std::net::SocketAddr;

use metrics::counter;
use vector_lib::internal_event::{error_stage, error_type, InternalEvent};

use crate::{internal_events::SocketOutgoingConnectionError, tls::TlsError};

#[derive(Debug)]
pub struct TcpSocketConnectionEstablished {
    pub peer_addr: Option<SocketAddr>,
}

impl InternalEvent for TcpSocketConnectionEstablished {
    fn emit(self) {
        if let Some(peer_addr) = self.peer_addr {
            debug!(message = "Connected.", %peer_addr);
        } else {
            debug!(message = "Connected.", peer_addr = "unknown");
        }
        counter!("connection_established_total", "mode" => "tcp").increment(1);
    }
}

#[derive(Debug)]
pub struct TcpSocketOutgoingConnectionError<E> {
    pub error: E,
}

impl<E: std::error::Error> InternalEvent for TcpSocketOutgoingConnectionError<E> {
    fn emit(self) {
        // ## skip check-duplicate-events ##
        // ## skip check-validity-events ##
        emit!(SocketOutgoingConnectionError { error: self.error });
    }
}

#[derive(Debug)]
pub struct TcpSocketConnectionShutdown;

impl InternalEvent for TcpSocketConnectionShutdown {
    fn emit(self) {
        warn!(message = "Received EOF from the server, shutdown.");
        counter!("connection_shutdown_total", "mode" => "tcp").increment(1);
    }
}

#[cfg(all(unix, feature = "sources-dnstap"))]
#[derive(Debug)]
pub struct TcpSocketError<'a, E> {
    pub(crate) error: &'a E,
    pub peer_addr: SocketAddr,
}

#[cfg(all(unix, feature = "sources-dnstap"))]
impl<E: std::fmt::Display> InternalEvent for TcpSocketError<'_, E> {
    fn emit(self) {
        error!(
            message = "TCP socket error.",
            error = %self.error,
            peer_addr = ?self.peer_addr,
            error_type = error_type::CONNECTION_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total",
            "error_type" => error_type::CONNECTION_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);
    }
}

#[derive(Debug)]
pub struct TcpSocketTlsConnectionError {
    pub error: TlsError,
}

impl InternalEvent for TcpSocketTlsConnectionError {
    fn emit(self) {
        match self.error {
            // Specific error that occurs when the other side is only
            // doing SYN/SYN-ACK connections for healthcheck.
            // https://github.com/vectordotdev/vector/issues/7318
            TlsError::Handshake { ref source }
                if source.code() == openssl::ssl::ErrorCode::SYSCALL
                    && source.io_error().is_none() =>
            {
                debug!(
                    message = "Connection error, probably a healthcheck.",
                    error = %self.error,
                    internal_log_rate_limit = true,
                );
            }
            _ => {
                error!(
                    message = "Connection error.",
                    error = %self.error,
                    error_code = "connection_failed",
                    error_type = error_type::WRITER_FAILED,
                    stage = error_stage::SENDING,
                    internal_log_rate_limit = true,
                );
                counter!(
                    "component_errors_total",
                    "error_code" => "connection_failed",
                    "error_type" => error_type::WRITER_FAILED,
                    "stage" => error_stage::SENDING,
                    "mode" => "tcp",
                )
                .increment(1);
            }
        }
    }
}

#[derive(Debug)]
pub struct TcpSendAckError {
    pub error: std::io::Error,
}

impl InternalEvent for TcpSendAckError {
    fn emit(self) {
        error!(
            message = "Error writing acknowledgement, dropping connection.",
            error = %self.error,
            error_code = "ack_failed",
            error_type = error_type::WRITER_FAILED,
            stage = error_stage::SENDING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total",
            "error_code" => "ack_failed",
            "error_type" => error_type::WRITER_FAILED,
            "stage" => error_stage::SENDING,
            "mode" => "tcp",
        )
        .increment(1);
    }
}

#[derive(Debug)]
pub struct TcpBytesReceived {
    pub byte_size: usize,
    pub peer_addr: SocketAddr,
}

impl InternalEvent for TcpBytesReceived {
    fn emit(self) {
        trace!(
            message = "Bytes received.",
            protocol = "tcp",
            byte_size = %self.byte_size,
            peer_addr = %self.peer_addr,
        );
        counter!(
            "component_received_bytes_total", "protocol" => "tcp"
        )
        .increment(self.byte_size as u64);
    }
}
