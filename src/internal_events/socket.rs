use metrics::counter;
use vector_lib::internal_event::{ComponentEventsDropped, InternalEvent, UNINTENTIONAL};
use vector_lib::{
    internal_event::{error_stage, error_type},
    json_size::JsonSize,
};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[allow(dead_code)] // some features only use some variants
pub enum SocketMode {
    Tcp,
    Udp,
    Unix,
}

impl SocketMode {
    pub const fn as_str(self) -> &'static str {
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
    pub byte_size: JsonSize,
    pub count: usize,
}

impl InternalEvent for SocketEventsReceived {
    fn emit(self) {
        let mode = self.mode.as_str();
        trace!(
            message = "Events received.",
            count = self.count,
            byte_size = self.byte_size.get(),
            %mode,
        );
        counter!("component_received_events_total", self.count as u64, "mode" => mode);
        counter!("component_received_event_bytes_total", self.byte_size.get() as u64, "mode" => mode);
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
    pub byte_size: JsonSize,
}

impl InternalEvent for SocketEventsSent {
    fn emit(self) {
        trace!(message = "Events sent.", count = %self.count, byte_size = %self.byte_size.get());
        counter!("component_sent_events_total", self.count, "mode" => self.mode.as_str());
        counter!("component_sent_event_bytes_total", self.byte_size.get() as u64, "mode" => self.mode.as_str());
    }
}

#[derive(Debug)]
pub struct SocketBindError<E> {
    pub mode: SocketMode,
    pub error: E,
}

impl<E: std::fmt::Display> InternalEvent for SocketBindError<E> {
    fn emit(self) {
        let mode = self.mode.as_str();
        error!(
            message = "Error binding socket.",
            error = %self.error,
            error_code = "socket_bind",
            error_type = error_type::IO_FAILED,
            stage = error_stage::RECEIVING,
            %mode,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total", 1,
            "error_code" => "socket_bind",
            "error_type" => error_type::IO_FAILED,
            "stage" => error_stage::RECEIVING,
            "mode" => mode,
        );
    }
}

#[derive(Debug)]
pub struct SocketReceiveError<E> {
    pub mode: SocketMode,
    pub error: E,
}

impl<E: std::fmt::Display> InternalEvent for SocketReceiveError<E> {
    fn emit(self) {
        let mode = self.mode.as_str();
        error!(
            message = "Error receiving data.",
            error = %self.error,
            error_code = "socket_receive",
            error_type = error_type::READER_FAILED,
            stage = error_stage::RECEIVING,
            %mode,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total", 1,
            "error_code" => "socket_receive",
            "error_type" => error_type::READER_FAILED,
            "stage" => error_stage::RECEIVING,
            "mode" => mode,
        );
    }
}

#[derive(Debug)]
pub struct SocketSendError<E> {
    pub mode: SocketMode,
    pub error: E,
}

impl<E: std::fmt::Display> InternalEvent for SocketSendError<E> {
    fn emit(self) {
        let mode = self.mode.as_str();
        let reason = "Error sending data.";
        error!(
            message = reason,
            error = %self.error,
            error_code = "socket_send",
            error_type = error_type::WRITER_FAILED,
            stage = error_stage::SENDING,
            %mode,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total", 1,
            "error_code" => "socket_send",
            "error_type" => error_type::WRITER_FAILED,
            "stage" => error_stage::SENDING,
            "mode" => mode,
        );

        emit!(ComponentEventsDropped::<UNINTENTIONAL> { count: 1, reason });
    }
}
