use super::InternalEvent;
use metrics::counter;

pub struct UnixSocketConnectionEstablished<'a> {
    pub path: &'a std::path::Path,
}

impl InternalEvent for UnixSocketConnectionEstablished<'_> {
    fn emit_logs(&self) {
        debug!(message = "connected", ?self.path);
    }

    fn emit_metrics(&self) {
        counter!("unix_socket_connections_established", 1,
            "component_kind" => "sink",
        );
    }
}

pub struct UnixSocketConnectionFailure<'a> {
    pub error: std::io::Error,
    pub path: &'a std::path::Path,
}

impl InternalEvent for UnixSocketConnectionFailure<'_> {
    fn emit_logs(&self) {
        error!(
            message = "unix socket connection failure",
            %self.error,
            ?self.path
        );
    }

    fn emit_metrics(&self) {
        counter!("unix_socket_connection_failures", 1,
            "component_kind" => "sink",
        );
    }
}

pub struct UnixSocketError<'a> {
    pub error: std::io::Error,
    pub path: &'a std::path::Path,
}

impl InternalEvent for UnixSocketError<'_> {
    fn emit_logs(&self) {
        debug!(message = "unix socket error.", %self.error, ?self.path);
    }

    fn emit_metrics(&self) {
        counter!("unix_socket_errors", 1);
    }
}

pub struct UnixSocketEventSent {
    pub byte_size: usize,
}

impl InternalEvent for UnixSocketEventSent {
    fn emit_metrics(&self) {
        counter!("unix_socket_events_sent", 1,
            "component_kind" => "sink",
        );
        counter!("unix_socket_bytes_sent", self.byte_size as u64,
            "component_kind" => "sink",
        );
    }
}

pub struct UnixSocketEventReceived {
    pub byte_size: usize,
}

impl InternalEvent for UnixSocketEventReceived {
    fn emit_logs(&self) {
        trace!(message = "Received one event.");
    }

    fn emit_metrics(&self) {
        counter!("events_received", 1,
            "component_kind" => "source",
            "component_type" => "socket",
            "mode" => "unix",
        );
        counter!("bytes_received", self.byte_size as u64,
            "component_kind" => "source",
            "component_type" => "socket",
            "mode" => "unix",
        );
    }
}
