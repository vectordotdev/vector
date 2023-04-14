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
