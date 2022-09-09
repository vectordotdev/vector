use metrics::counter;
use vector_core::internal_event::InternalEvent;

use vector_common::internal_event::{error_stage, error_type};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[allow(dead_code)] // some features only use some variants
pub enum SocketMode {
    Tcp,
    Udp,
    Unix,
}

impl SocketMode {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Tcp => "tcp",
            Self::Udp => "udp",
            Self::Unix => "unix",
        }
    }
}
#[derive(Debug)]
pub struct SocketBytesReceived {
    pub mode: SocketMode,
    pub byte_size: usize,
}

impl InternalEvent for SocketBytesReceived {
    fn emit(self) {
        let protocol = self.mode.as_str();
        trace!(
            message = "Bytes received.",
            byte_size = %self.byte_size,
            %protocol,
        );
        counter!(
            "component_received_bytes_total", self.byte_size as u64,
            "protocol" => protocol,
        );
    }
}

#[derive(Debug)]
pub struct SocketEventsReceived {
    pub mode: SocketMode,
    pub byte_size: usize,
    pub count: usize,
}

impl InternalEvent for SocketEventsReceived {
    fn emit(self) {
        let mode = self.mode.as_str();
        trace!(
            message = "Events received.",
            count = self.count,
            byte_size = self.byte_size,
            %mode,
        );
        counter!("component_received_events_total", self.count as u64, "mode" => mode);
        counter!("component_received_event_bytes_total", self.byte_size as u64, "mode" => mode);
        // deprecated
        counter!("events_in_total", self.count as u64, "mode" => mode);
    }
}

#[derive(Debug)]
pub struct SocketBytesSent {
    pub mode: SocketMode,
    pub byte_size: usize,
}

impl InternalEvent for SocketBytesSent {
    fn emit(self) {
        let protocol = self.mode.as_str();
        trace!(
            message = "Bytes sent.",
            byte_size = %self.byte_size,
            %protocol,
        );
        counter!(
            "component_sent_bytes_total", self.byte_size as u64,
            "protocol" => protocol,
        );
    }
}

#[derive(Debug)]
pub struct SocketEventsSent {
    pub mode: SocketMode,
    pub count: u64,
    pub byte_size: usize,
}

impl InternalEvent for SocketEventsSent {
    fn emit(self) {
        trace!(message = "Events sent.", count = %self.count, byte_size = %self.byte_size);
        counter!("component_sent_events_total", self.count as u64, "mode" => self.mode.as_str());
        counter!("component_sent_event_bytes_total", self.byte_size as u64, "mode" => self.mode.as_str());
    }
}

#[derive(Debug)]
pub struct SocketReceiveError<'a> {
    pub mode: SocketMode,
    pub error: &'a codecs::decoding::Error,
}

impl<'a> InternalEvent for SocketReceiveError<'a> {
    fn emit(self) {
        let mode = self.mode.as_str();
        error!(
            message = "Error receiving data.",
            error = %self.error,
            error_code = "receiving_data",
            error_type = error_type::CONNECTION_FAILED,
            stage = error_stage::RECEIVING,
            %mode,
        );
        counter!(
            "component_errors_total", 1,
            "error_code" => "receiving_data",
            "error_type" => error_type::CONNECTION_FAILED,
            "stage" => error_stage::RECEIVING,
            "mode" => mode,
        );
        // deprecated
        counter!("connection_errors_total", 1, "mode" => mode);
    }
}
