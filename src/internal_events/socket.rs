use super::prelude::{error_stage, error_type};
use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug, Clone, Copy)]
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
pub struct SocketEventsReceived {
    pub mode: SocketMode,
    pub byte_size: usize,
    pub count: usize,
}

impl InternalEvent for SocketEventsReceived {
    fn emit(self) {
        trace!(
            message = "Events received.",
            count = self.count,
            byte_size = self.byte_size,
            mode = self.mode.as_str()
        );
        counter!("component_received_events_total", self.count as u64, "mode" => self.mode.as_str());
        counter!("component_received_event_bytes_total", self.byte_size as u64, "mode" => self.mode.as_str());
        // deprecated
        counter!("events_in_total", self.count as u64, "mode" => self.mode.as_str());
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
        error!(
            message = "Error receiving data.",
            error = %self.error,
            error_code = "receiving_data",
            error_type = error_type::CONNECTION_FAILED,
            stage = error_stage::RECEIVING,
            mode = %self.mode.as_str(),
        );
        counter!(
            "component_errors_total", 1,
            "error_code" => "receiving_data",
            "error_type" => error_type::CONNECTION_FAILED,
            "stage" => error_stage::RECEIVING,
            "mode" => self.mode.as_str(),
        );
        // deprecated
        counter!("connection_errors_total", 1, "mode" => self.mode.as_str());
    }
}
