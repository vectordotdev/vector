use std::net::SocketAddr;

use vector_lib::{
    NamedInternalEvent, counter,
    internal_event::{CounterName, InternalEvent, error_stage, error_type},
};

use crate::{internal_events::SocketOutgoingConnectionError, tls::TlsError};

#[derive(Debug, NamedInternalEvent)]
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
        counter!(CounterName::ConnectionEstablishedTotal, "mode" => "tcp").increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
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

#[derive(Debug, NamedInternalEvent)]
pub struct TcpSocketConnectionShutdown;

impl InternalEvent for TcpSocketConnectionShutdown {
    fn emit(self) {
        warn!(message = "Received EOF from the server, shutdown.");
        counter!(CounterName::ConnectionShutdownTotal, "mode" => "tcp").increment(1);
    }
}

/// Emitted once per accepted TCP source connection, after the per-connection
/// task exits — regardless of cause. This includes pre-loop exits (TLS
/// handshake failure, shutdown signal arriving during handshake) as well as
/// every read/ack loop exit (graceful peer EOF, decoder failure, downstream
/// closed, ack write failure, shutdown signal, tripwire, max connection
/// duration). Pairs exactly with `ConnectionOpen`.
#[derive(Debug, NamedInternalEvent)]
pub struct TcpSourceConnectionClosed;

impl InternalEvent for TcpSourceConnectionClosed {
    fn emit(self) {
        debug!(message = "Connection closed.");
        counter!(CounterName::ConnectionShutdownTotal, "mode" => "tcp").increment(1);
    }
}

#[cfg(all(unix, feature = "sources-dnstap"))]
#[derive(Debug, NamedInternalEvent)]
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
        );
        counter!(
            CounterName::ComponentErrorsTotal,
            "error_type" => error_type::CONNECTION_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
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
                );
            }
            _ => {
                error!(
                    message = "Connection error.",
                    error = %self.error,
                    error_code = "connection_failed",
                    error_type = error_type::WRITER_FAILED,
                    stage = error_stage::SENDING,
                );
                counter!(
                    CounterName::ComponentErrorsTotal,
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

#[derive(Debug, NamedInternalEvent)]
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
        );
        counter!(
            CounterName::ComponentErrorsTotal,
            "error_code" => "ack_failed",
            "error_type" => error_type::WRITER_FAILED,
            "stage" => error_stage::SENDING,
            "mode" => "tcp",
        )
        .increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
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
            CounterName::ComponentReceivedBytesTotal, "protocol" => "tcp"
        )
        .increment(self.byte_size as u64);
    }
}

#[cfg(test)]
mod tests {
    use std::io;

    use serial_test::serial;
    use vector_lib::event::MetricValue;
    use vector_lib::internal_event::InternalEvent;
    use vector_lib::metrics::Controller;

    use super::{TcpSendAckError, TcpSourceConnectionClosed};

    /// Returns the current value of a counter matching `name` and all `tags`.
    /// Counters that have not yet been touched aren't in the snapshot and
    /// register as 0.0 here.
    fn counter_value(name: &str, tags: &[(&str, &str)]) -> f64 {
        Controller::get()
            .expect("metrics controller initialized")
            .capture_metrics()
            .into_iter()
            .find(|m| {
                m.name() == name
                    && tags
                        .iter()
                        .all(|(k, v)| m.tags().is_some_and(|t| t.get(k) == Some(*v)))
            })
            .map(|m| match m.value() {
                MetricValue::Counter { value } => *value,
                other => panic!("expected counter for {name}, got {other:?}"),
            })
            .unwrap_or(0.0)
    }

    /// `TcpSourceConnectionClosed` MUST bump `connection_shutdown_total{mode="tcp"}`
    /// once per emit. Pins the contract that this event is the sole owner of the
    /// connection-close counter on the source side.
    #[test]
    #[serial]
    fn tcp_source_connection_closed_increments_shutdown_total() {
        crate::test_util::trace_init();
        let before = counter_value("connection_shutdown_total", &[("mode", "tcp")]);

        TcpSourceConnectionClosed.emit();

        let after = counter_value("connection_shutdown_total", &[("mode", "tcp")]);
        assert_eq!(after - before, 1.0);
    }

    /// `TcpSendAckError` is an `*Error` event and per the instrumentation spec MUST
    /// only emit on real errors — bumping `component_errors_total` with the
    /// `ack_failed` error_code.
    #[test]
    #[serial]
    fn tcp_send_ack_error_emit_always_increments_component_errors_total() {
        crate::test_util::trace_init();
        let errors_before = counter_value(
            "component_errors_total",
            &[
                ("error_code", "ack_failed"),
                ("error_type", "writer_failed"),
                ("stage", "sending"),
                ("mode", "tcp"),
            ],
        );
        let shutdown_before = counter_value("connection_shutdown_total", &[("mode", "tcp")]);

        TcpSendAckError {
            error: io::Error::from(io::ErrorKind::ConnectionReset),
        }
        .emit();

        assert_eq!(
            counter_value(
                "component_errors_total",
                &[
                    ("error_code", "ack_failed"),
                    ("error_type", "writer_failed"),
                    ("stage", "sending"),
                    ("mode", "tcp"),
                ],
            ) - errors_before,
            1.0,
        );
        assert_eq!(
            counter_value("connection_shutdown_total", &[("mode", "tcp")]),
            shutdown_before,
            "TcpSendAckError must not bump the connection-close counter — \
             that is TcpSourceConnectionClosed's responsibility.",
        );
    }
}
