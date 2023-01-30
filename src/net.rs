use std::os::fd::AsRawFd;

use socket2::SockRef;

/// Sets the receive buffer size for a socket.
///
/// This is the equivalent of setting the `SO_RCVBUF` socket setting directly.
///
/// # Errors
///
/// If there is an error setting the receive buffer size on the given socket, or if the value given
/// as the socket is not a valid socket, an error variant will be returned explaining the underlying
/// I/O error.
pub fn set_receive_buffer_size<S>(socket: &S, size: usize) -> std::io::Result<()>
where
    S: AsRawFd,
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
pub fn set_send_buffer_size<S>(socket: &S, size: usize) -> std::io::Result<()>
where
    S: AsRawFd,
{
    SockRef::from(socket).set_send_buffer_size(size)
}
