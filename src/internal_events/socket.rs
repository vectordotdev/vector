// ## skip check-events ##

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
    fn emit_logs(&self) {
        trace!(
            message = "Received events.",
            count = self.count,
            byte_size = self.byte_size,
            mode = self.mode.as_str()
        );
    }

    fn emit_metrics(&self) {
        counter!("component_received_events_total", 1, "mode" => self.mode.as_str());
        counter!("events_in_total", 1, "mode" => self.mode.as_str());
        counter!("processed_bytes_total", self.byte_size as u64, "mode" => self.mode.as_str());
    }
}

#[derive(Debug)]
pub struct SocketEventsSent {
    pub mode: SocketMode,
    pub count: u64,
    pub byte_size: usize,
}

impl InternalEvent for SocketEventsSent {
    fn emit_logs(&self) {
        trace!(message = "Events sent.", count = %self.count, byte_size = %self.byte_size);
    }

    fn emit_metrics(&self) {
        counter!("processed_bytes_total", self.byte_size as u64, "mode" => self.mode.as_str());
    }
}

#[cfg(feature = "codecs")]
#[derive(Debug)]
pub struct SocketReceiveError<'a> {
    pub mode: SocketMode,
    pub error: &'a crate::codecs::Error,
}

#[cfg(feature = "codecs")]
impl<'a> InternalEvent for SocketReceiveError<'a> {
    fn emit_logs(&self) {
        error!(message = "Error receiving data.", error = ?self.error, mode = %self.mode.as_str());
    }

    fn emit_metrics(&self) {
        counter!("connection_errors_total", 1, "mode" => self.mode.as_str());
    }
}
