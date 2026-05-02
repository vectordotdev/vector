//! Internal events for NetFlow source monitoring and debugging.

use metrics::counter;
use std::net::SocketAddr;
use tracing::{debug, error};
use vector_lib::internal_event::{error_stage, error_type, InternalEvent};

/// NetFlow packet received successfully.
#[derive(Debug)]
pub struct NetflowEventsReceived {
    pub count: usize,
    pub byte_size: usize,
    pub peer_addr: SocketAddr,
}

impl InternalEvent for NetflowEventsReceived {
    fn emit(self) {
        debug!(
            message = "NetFlow events received.",
            count = self.count,
            byte_size = self.byte_size,
            peer_addr = %self.peer_addr,
        );

        counter!(
            "component_received_events_total",
            "peer_addr" => self.peer_addr.ip().to_string(),
        )
        .increment(self.count as u64);

        counter!(
            "component_received_event_bytes_total",
            "peer_addr" => self.peer_addr.ip().to_string(),
        )
        .increment(self.byte_size as u64);
    }
}

/// NetFlow packet parsing failed.
#[derive(Debug)]
pub struct NetflowParseError<'a> {
    pub error: &'a str,
    pub protocol: &'a str,
    pub peer_addr: SocketAddr,
}

impl<'a> InternalEvent for NetflowParseError<'a> {
    fn emit(self) {
        error!(
            message = "Failed to parse NetFlow packet.",
            error = %self.error,
            protocol = %self.protocol,
            peer_addr = %self.peer_addr,
            error_code = "parse_failed",
            error_type = error_type::PARSER_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_limit = true,
        );

        counter!(
            "component_errors_total",
            "error_code" => "parse_failed",
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::PROCESSING,
            "protocol" => self.protocol.to_string(),
            "peer_addr" => self.peer_addr.ip().to_string(),
        )
        .increment(1);
    }
}

/// Socket binding error.
#[derive(Debug)]
pub struct NetflowBindError {
    pub address: SocketAddr,
    pub error: std::io::Error,
}

impl InternalEvent for NetflowBindError {
    fn emit(self) {
        error!(
            message = "Failed to bind NetFlow socket.",
            address = %self.address,
            error = %self.error,
            error_code = "socket_bind_failed",
            error_type = error_type::CONNECTION_FAILED,
            stage = error_stage::RECEIVING,
        );

        counter!(
            "component_errors_total",
            "error_code" => "socket_bind_failed",
            "error_type" => error_type::CONNECTION_FAILED,
            "stage" => error_stage::RECEIVING,
            "address" => self.address.ip().to_string(),
        )
        .increment(1);
    }
}

/// Socket receive error.
#[derive(Debug)]
pub struct NetflowReceiveError {
    pub error: std::io::Error,
}

impl InternalEvent for NetflowReceiveError {
    fn emit(self) {
        error!(
            message = "Failed to receive NetFlow packet.",
            error = %self.error,
            error_code = "socket_receive_failed",
            error_type = error_type::CONNECTION_FAILED,
            stage = error_stage::RECEIVING,
            internal_log_rate_limit = true,
        );

        counter!(
            "component_errors_total",
            "error_code" => "socket_receive_failed",
            "error_type" => error_type::CONNECTION_FAILED,
            "stage" => error_stage::RECEIVING,
        )
        .increment(1);
    }
}

/// Emitted when the configured protocol list rejects a packet (disabled name or unknown version).
#[derive(Debug)]
pub struct ProtocolDisabled {
    pub protocol: &'static str,
    pub peer_addr: SocketAddr,
}

impl InternalEvent for ProtocolDisabled {
    fn emit(self) {
        debug!(
            message = "Protocol disabled, ignoring packet.",
            protocol = self.protocol,
            peer_addr = %self.peer_addr,
        );
    }
}

/// Emitted after a successful parse of at least one event from a datagram.
#[derive(Debug)]
pub struct ProtocolParseSuccess {
    pub protocol: &'static str,
    pub peer_addr: SocketAddr,
    pub event_count: usize,
    pub byte_size: usize,
}

impl InternalEvent for ProtocolParseSuccess {
    fn emit(self) {
        debug!(
            message = "Protocol parsed successfully.",
            protocol = self.protocol,
            peer_addr = %self.peer_addr,
            event_count = self.event_count,
            byte_size = self.byte_size,
        );
    }
}
