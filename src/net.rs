//! Networking-related helper functions.

use std::{io, time::Duration};

use socket2::{SockRef, TcpKeepalive};
use tokio::net::TcpStream;

/// Sets the receive buffer size for a socket.
///
/// This is the equivalent of setting the `SO_RCVBUF` socket setting directly.
///
/// # Errors
///
/// If there is an error setting the receive buffer size on the given socket, or if the value given
/// as the socket is not a valid socket, an error variant will be returned explaining the underlying
/// I/O error.
pub fn set_receive_buffer_size<'s, S>(socket: &'s S, size: usize) -> io::Result<()>
where
    SockRef<'s>: From<&'s S>,
{
    SockRef::from(socket).set_recv_buffer_size(size)
}

/// Sets the send buffer size for a socket.
///
/// This is the equivalent of setting the `SO_SNDBUF` socket setting directly.
///
/// # Errors
///
/// If there is an error setting the send buffer size on the given socket, or if the value given
/// as the socket is not a valid socket, an error variant will be returned explaining the underlying
/// I/O error.
pub fn set_send_buffer_size<'s, S>(socket: &'s S, size: usize) -> io::Result<()>
where
    SockRef<'s>: From<&'s S>,
{
    SockRef::from(socket).set_send_buffer_size(size)
}

/// Sets the TCP keepalive behavior on a socket.
///
/// This is the equivalent of setting the `SO_KEEPALIVE` and `TCP_KEEPALIVE` socket settings
/// directly.
///
/// # Errors
///
/// If there is an error with either enabling keepalive probes or setting the TCP keepalive idle
/// timeout on the given socket, an error variant will be returned explaining the underlying I/O
/// error.
pub fn set_keepalive(socket: &TcpStream, ttl: Duration) -> io::Result<()> {
    SockRef::from(socket).set_tcp_keepalive(&TcpKeepalive::new().with_time(ttl))
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
pub fn is_graceful_tls_shutdown(err: &io::Error) -> bool {
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

#[cfg(test)]
mod tests {
    use std::pin::Pin;

    use openssl::ssl::{SslAcceptor, SslConnector, SslFiletype, SslMethod, SslVerifyMode};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio_openssl::SslStream;

    use crate::tls::{TEST_PEM_CA_PATH, TEST_PEM_CRT_PATH, TEST_PEM_KEY_PATH};

    use super::{TcpStream, io, is_graceful_tls_shutdown};

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
}
