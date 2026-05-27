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
pub struct TcpSourceConnectionShutdown;

impl InternalEvent for TcpSourceConnectionShutdown {
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
        if is_graceful_tls_shutdown(&self.error) {
            // The peer cleanly closed its TLS session (e.g. a rolling pod) before we
            // could send the acknowledgement. This is a lifecycle event, not an error
            // — log it at warn and skip the component_errors_total increment. The
            // connection_shutdown_total counter is bumped by the unified
            // TcpSourceConnectionShutdown emit on the per-connection task's exit
            // (paired with ConnectionOpen), so we don't bump it here.
            warn!(
                message = "Connection closed by peer before acknowledgement could be sent.",
                error = %self.error,
            );
            return;
        }
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

// SSL_R_PROTOCOL_IS_SHUTDOWN from openssl/include/openssl/sslerr.h. Stable across
// OpenSSL 1.1.1 and 3.x. Not re-exported by the `openssl-sys` crate so we pin it here.
const SSL_R_PROTOCOL_IS_SHUTDOWN: std::ffi::c_int = 207;

/// Returns true when an `io::Error` represents a peer-initiated, graceful TLS
/// shutdown (close_notify), rather than a real I/O failure.
///
/// Two cases are recognized:
/// - `SSL_ERROR_ZERO_RETURN`: the peer sent `close_notify` and we observed it
///   during this I/O call.
/// - `SSL_R_PROTOCOL_IS_SHUTDOWN`: a subsequent write after the session was
///   already shut down ("ssl session has been shut down").
fn is_graceful_tls_shutdown(err: &std::io::Error) -> bool {
    let Some(ssl) = err
        .get_ref()
        .and_then(|inner| inner.downcast_ref::<openssl::ssl::Error>())
    else {
        return false;
    };
    if ssl.code() == openssl::ssl::ErrorCode::ZERO_RETURN {
        return true;
    }
    ssl.ssl_error().is_some_and(|stack| {
        stack
            .errors()
            .iter()
            .any(|e| e.reason_code() == SSL_R_PROTOCOL_IS_SHUTDOWN)
    })
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
    use std::pin::Pin;

    use crate::tls::{TEST_PEM_CA_PATH, TEST_PEM_CRT_PATH, TEST_PEM_KEY_PATH};
    use openssl::ssl::{SslAcceptor, SslConnector, SslFiletype, SslMethod, SslVerifyMode};
    use serial_test::serial;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};
    use tokio_openssl::SslStream;
    use vector_lib::event::MetricValue;
    use vector_lib::internal_event::InternalEvent;
    use vector_lib::metrics::Controller;

    use super::{TcpSendAckError, TcpSourceConnectionShutdown, is_graceful_tls_shutdown};

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

    #[test]
    fn plain_io_errors_are_not_graceful() {
        for err in [
            io::Error::from(io::ErrorKind::BrokenPipe),
            io::Error::from(io::ErrorKind::ConnectionReset),
            io::Error::from(io::ErrorKind::UnexpectedEof),
            io::Error::other("not an ssl error"),
        ] {
            assert!(
                !is_graceful_tls_shutdown(&err),
                "expected non-graceful, got graceful for {err:?}",
            );
        }
    }

    // Drives a real TLS handshake between two local sockets and completes a
    // bidirectional SSL shutdown. A subsequent write surfaces a `std::io::Error`
    // wrapping an `openssl::ssl::Error` from the same code path production hits,
    // validating that the helper correctly identifies it as a graceful shutdown
    // — without having to synthesize an `openssl::ssl::Error` (whose fields are
    // crate-private). Bidirectional shutdown is what reliably elicits
    // SSL_R_PROTOCOL_IS_SHUTDOWN; a half-closed session would still permit
    // writes per RFC 5246.
    #[tokio::test]
    async fn detects_graceful_shutdown_from_real_ssl_stream() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut acceptor = SslAcceptor::mozilla_intermediate(SslMethod::tls()).unwrap();
            acceptor
                .set_private_key_file(TEST_PEM_KEY_PATH, SslFiletype::PEM)
                .unwrap();
            acceptor
                .set_certificate_chain_file(TEST_PEM_CRT_PATH)
                .unwrap();
            let acceptor = acceptor.build();
            let ssl = openssl::ssl::Ssl::new(acceptor.context()).unwrap();
            let mut tls = SslStream::new(ssl, stream).unwrap();
            Pin::new(&mut tls).accept().await.unwrap();
            // Cleanly close the SSL session — sends close_notify and waits for the peer's.
            Pin::new(&mut tls).shutdown().await.unwrap();
        });

        let mut connector = SslConnector::builder(SslMethod::tls()).unwrap();
        connector.set_ca_file(TEST_PEM_CA_PATH).unwrap();
        connector.set_verify(SslVerifyMode::NONE);
        let ssl = connector
            .build()
            .configure()
            .unwrap()
            .into_ssl("localhost")
            .unwrap();
        let stream = TcpStream::connect(addr).await.unwrap();
        let mut tls = SslStream::new(ssl, stream).unwrap();
        Pin::new(&mut tls).connect().await.unwrap();

        // Drain the server's close_notify so our SSL state observes the peer shutdown.
        let mut buf = [0u8; 1];
        let n = tls.read(&mut buf).await.unwrap();
        assert_eq!(n, 0, "expected EOF from peer's close_notify");

        // Complete the bidirectional SSL shutdown locally. Once both sides are
        // shut down, OpenSSL marks the session as SHUTDOWN and any further write
        // returns SSL_R_PROTOCOL_IS_SHUTDOWN ("ssl session has been shut down").
        Pin::new(&mut tls).shutdown().await.unwrap();

        let err = tls
            .write_all(b"too late")
            .await
            .expect_err("write after bidirectional shutdown should fail");

        assert!(
            is_graceful_tls_shutdown(&err),
            "expected graceful shutdown detection, got: {err:?} (inner: {:?})",
            err.get_ref(),
        );

        server.await.unwrap();
    }

    /// Drives a real bidirectional SSL shutdown to elicit a genuine
    /// `openssl::ssl::Error`, returning the wrapping `io::Error` from the same
    /// code path Vector hits in production. Used by the ack-error metric tests
    /// to avoid synthesizing an `openssl::ssl::Error` (its fields are crate-private).
    async fn make_graceful_tls_io_error() -> io::Error {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut acceptor = SslAcceptor::mozilla_intermediate(SslMethod::tls()).unwrap();
            acceptor
                .set_private_key_file(TEST_PEM_KEY_PATH, SslFiletype::PEM)
                .unwrap();
            acceptor
                .set_certificate_chain_file(TEST_PEM_CRT_PATH)
                .unwrap();
            let acceptor = acceptor.build();
            let ssl = openssl::ssl::Ssl::new(acceptor.context()).unwrap();
            let mut tls = SslStream::new(ssl, stream).unwrap();
            Pin::new(&mut tls).accept().await.unwrap();
            Pin::new(&mut tls).shutdown().await.unwrap();
        });

        let mut connector = SslConnector::builder(SslMethod::tls()).unwrap();
        connector.set_ca_file(TEST_PEM_CA_PATH).unwrap();
        connector.set_verify(SslVerifyMode::NONE);
        let ssl = connector
            .build()
            .configure()
            .unwrap()
            .into_ssl("localhost")
            .unwrap();
        let stream = TcpStream::connect(addr).await.unwrap();
        let mut tls = SslStream::new(ssl, stream).unwrap();
        Pin::new(&mut tls).connect().await.unwrap();

        let mut buf = [0u8; 1];
        let _ = tls.read(&mut buf).await.unwrap();
        Pin::new(&mut tls).shutdown().await.unwrap();
        let err = tls.write_all(b"too late").await.expect_err("write fails");
        server.await.unwrap();
        err
    }

    /// The post-loop emit in `handle_stream` MUST bump
    /// `connection_shutdown_total{mode="tcp"}` once per accepted connection.
    /// This pins the contract that the unified counter is owned by this event.
    #[test]
    #[serial]
    fn tcp_source_connection_shutdown_increments_shutdown_total() {
        crate::test_util::trace_init();
        let before = counter_value("connection_shutdown_total", &[("mode", "tcp")]);

        TcpSourceConnectionShutdown.emit();

        let after = counter_value("connection_shutdown_total", &[("mode", "tcp")]);
        assert_eq!(after - before, 1.0);
    }

    /// Graceful TLS shutdown during ack write must not bump any counter — the
    /// connection-shutdown increment is owned by `TcpSourceConnectionShutdown`
    /// on the per-connection task's exit (paired with `ConnectionOpen`), and
    /// the case is not an error so `component_errors_total` must stay flat
    /// too. This is what the customer PR is actually fixing on the metrics side.
    #[tokio::test]
    #[serial]
    async fn graceful_ack_error_does_not_increment_counters() {
        crate::test_util::trace_init();
        let shutdown_before = counter_value("connection_shutdown_total", &[("mode", "tcp")]);
        let errors_before = counter_value(
            "component_errors_total",
            &[("error_code", "ack_failed"), ("mode", "tcp")],
        );

        let err = make_graceful_tls_io_error().await;
        assert!(
            is_graceful_tls_shutdown(&err),
            "test prerequisite: io::Error must be a graceful TLS shutdown",
        );
        TcpSendAckError { error: err }.emit();

        assert_eq!(
            counter_value("connection_shutdown_total", &[("mode", "tcp")]),
            shutdown_before,
            "graceful TcpSendAckError must not bump connection_shutdown_total \
             (that counter is owned by TcpSourceConnectionShutdown)",
        );
        assert_eq!(
            counter_value(
                "component_errors_total",
                &[("error_code", "ack_failed"), ("mode", "tcp")],
            ),
            errors_before,
            "graceful TcpSendAckError must not bump component_errors_total",
        );
    }

    /// A non-graceful ack write failure (a plain io::Error, e.g. ECONNRESET)
    /// must still log at ERROR and bump `component_errors_total` with the
    /// `ack_failed` error_code — preserving the existing error contract for
    /// genuine ack write failures.
    #[test]
    #[serial]
    fn non_graceful_ack_error_increments_component_errors_total() {
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

        let err = io::Error::from(io::ErrorKind::ConnectionReset);
        assert!(!is_graceful_tls_shutdown(&err));
        TcpSendAckError { error: err }.emit();

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
            "non-graceful ack failure must not directly bump shutdown counter \
             (that's the source-loop-exit emit's job)",
        );
    }
}
