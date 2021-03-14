use serde::{Deserialize, Serialize};
#[cfg(unix)]
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd};
#[cfg(unix)]
use tokio::net::TcpStream;

/// Configuration for keepalive probes in a TCP stream.
///
/// This config's properties map to TCP keepalive properties in Tokio:
/// https://github.com/tokio-rs/tokio/blob/tokio-0.2.22/tokio/src/net/tcp/stream.rs#L516-L537
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TcpKeepaliveConfig {
    pub time_secs: Option<u64>,
}

#[cfg(unix)]
// This function will be obsolete after tokio/mio internally use `socket2` and expose the methods to
// apply options to a socket. Until then, use of `unsafe` is necessary here.
pub fn set_keepalive(socket: &TcpStream, params: &socket2::TcpKeepalive) {
    // SAFETY: We create a socket from an existing file descriptor without destructing the previous
    // owner and therefore temporarily have two objects that own the same socket.
    //
    // This is safe since we make sure that the new socket owner does not call its destructor by
    // giving up its ownership at the end of this scope.
    let socket = unsafe { socket2::Socket::from_raw_fd(socket.as_raw_fd()) };

    if let Err(error) = socket.set_tcp_keepalive(params) {
        warn!(message = "Failed configuring keepalive on TCP socket.", %error);
    }

    socket.into_raw_fd();
}

#[cfg(unix)]
// This function will be obsolete after tokio/mio internally use `socket2` and expose the methods to
// apply options to a socket. Until then, use of `unsafe` is necessary here.
pub fn set_receive_buffer_size(socket: &TcpStream, size: usize) {
    // SAFETY: We create a socket from an existing file descriptor without destructing the previous
    // owner and therefore temporarily have two objects that own the same socket.
    //
    // This is safe since we make sure that the new socket owner does not call its destructor by
    // giving up its ownership at the end of this scope.
    let socket = unsafe { socket2::Socket::from_raw_fd(socket.as_raw_fd()) };

    if let Err(error) = socket.set_recv_buffer_size(size) {
        warn!(message = "Failed configuring receive buffer size on TCP socket.", %error);
    }

    socket.into_raw_fd();
}

#[cfg(unix)]
// This function will be obsolete after tokio/mio internally use `socket2` and expose the methods to
// apply options to a socket. Until then, use of `unsafe` is necessary here.
pub fn set_send_buffer_size(socket: &TcpStream, size: usize) {
    // SAFETY: We create a socket from an existing file descriptor without destructing the previous
    // owner and therefore temporarily have two objects that own the same socket.
    //
    // This is safe since we make sure that the new socket owner does not call its destructor by
    // giving up its ownership at the end of this scope.
    let socket = unsafe { socket2::Socket::from_raw_fd(socket.as_raw_fd()) };

    if let Err(error) = socket.set_send_buffer_size(size) {
        warn!(message = "Failed configuring send buffer size on TCP socket.", %error);
    }

    socket.into_raw_fd();
}
