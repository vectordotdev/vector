use std::net::Ipv4Addr;

use vector_lib::{
    NamedInternalEvent, counter, histogram,
    internal_event::{
        ComponentEventsDropped, CounterName, HistogramName, InternalEvent, UNINTENTIONAL,
        error_stage, error_type,
    },
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

#[derive(Debug, NamedInternalEvent)]
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
            CounterName::ComponentReceivedBytesTotal,
            "protocol" => protocol,
        )
        .increment(self.byte_size as u64);
        histogram!(HistogramName::ComponentReceivedBytes).record(self.byte_size as f64);
    }
}

#[derive(Debug, NamedInternalEvent)]
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
        counter!(CounterName::ComponentReceivedEventsTotal, "mode" => mode)
            .increment(self.count as u64);
        counter!(CounterName::ComponentReceivedEventBytesTotal, "mode" => mode)
            .increment(self.byte_size.get() as u64);
        histogram!(HistogramName::ComponentReceivedBytes, "mode" => mode)
            .record(self.byte_size.get() as f64);
    }
}

#[derive(Debug, NamedInternalEvent)]
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
            CounterName::ComponentSentBytesTotal,
            "protocol" => protocol,
        )
        .increment(self.byte_size as u64);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct SocketEventsSent {
    pub mode: SocketMode,
    pub count: u64,
    pub byte_size: JsonSize,
}

impl InternalEvent for SocketEventsSent {
    fn emit(self) {
        trace!(message = "Events sent.", count = %self.count, byte_size = %self.byte_size.get());
        counter!(CounterName::ComponentSentEventsTotal, "mode" => self.mode.as_str())
            .increment(self.count);
        counter!(CounterName::ComponentSentEventBytesTotal, "mode" => self.mode.as_str())
            .increment(self.byte_size.get() as u64);
    }
}

#[derive(Debug, NamedInternalEvent)]
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
            stage = error_stage::INITIALIZING,
            %mode,
        );
        counter!(
            CounterName::ComponentErrorsTotal,
            "error_code" => "socket_bind",
            "error_type" => error_type::IO_FAILED,
            "stage" => error_stage::INITIALIZING,
            "mode" => mode,
        )
        .increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct SocketMulticastGroupJoinError<E> {
    pub error: E,
    pub group_addr: Ipv4Addr,
    pub interface: Ipv4Addr,
}

impl<E: std::fmt::Display> InternalEvent for SocketMulticastGroupJoinError<E> {
    fn emit(self) {
        // Multicast groups are only used in UDP mode
        let mode = SocketMode::Udp.as_str();
        let group_addr = self.group_addr.to_string();
        let interface = self.interface.to_string();

        error!(
            message = "Error joining multicast group.",
            error = %self.error,
            error_code = "socket_multicast_group_join",
            error_type = error_type::IO_FAILED,
            stage = error_stage::INITIALIZING,
            %mode,
            %group_addr,
            %interface,
        );
        counter!(
            CounterName::ComponentErrorsTotal,
            "error_code" => "socket_multicast_group_join",
            "error_type" => error_type::IO_FAILED,
            "stage" => error_stage::INITIALIZING,
            "mode" => mode,
            "group_addr" => group_addr,
            "interface" => interface,
        )
        .increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
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
        );
        counter!(
            CounterName::ComponentErrorsTotal,
            "error_code" => "socket_receive",
            "error_type" => error_type::READER_FAILED,
            "stage" => error_stage::RECEIVING,
            "mode" => mode,
        )
        .increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
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
        );
        counter!(
            CounterName::ComponentErrorsTotal,
            "error_code" => "socket_send",
            "error_type" => error_type::WRITER_FAILED,
            "stage" => error_stage::SENDING,
            "mode" => mode,
        )
        .increment(1);

        emit!(ComponentEventsDropped::<UNINTENTIONAL> { count: 1, reason });
    }
}
