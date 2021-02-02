#[cfg(unix)]
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd};
#[cfg(unix)]
use tokio::net::UdpSocket;

#[cfg(unix)]
// This function will be obsolete after tokio/mio internally use `socket2` and expose the methods to
// apply options to a socket. Until then, use of `unsafe` is necessary here.
pub fn set_receive_buffer_size(socket: &UdpSocket, size: usize) {
    // SAFETY: We create a socket from an existing file descriptor without destructing the previous
    // owner and therefore temporarily have two objects that own the same socket.
    //
    // This is safe since we make sure that the new socket owner does not call its destructor by
    // giving up its ownership at the end of this scope.
    let socket = unsafe { socket2::Socket::from_raw_fd(socket.as_raw_fd()) };

    if let Err(error) = socket.set_recv_buffer_size(size) {
        warn!(message = "Failed configuring receive buffer size on UDP socket.", %error);
    }

    socket.into_raw_fd();
}

#[cfg(unix)]
// This function will be obsolete after tokio/mio internally use `socket2` and expose the methods to
// apply options to a socket. Until then, use of `unsafe` is necessary here.
pub fn set_send_buffer_size(socket: &UdpSocket, size: usize) {
    // SAFETY: We create a socket from an existing file descriptor without destructing the previous
    // owner and therefore temporarily have two objects that own the same socket.
    //
    // This is safe since we make sure that the new socket owner does not call its destructor by
    // giving up its ownership at the end of this scope.
    let socket = unsafe { socket2::Socket::from_raw_fd(socket.as_raw_fd()) };

    if let Err(error) = socket.set_send_buffer_size(size) {
        warn!(message = "Failed configuring send buffer size on UDP socket.", %error);
    }

    socket.into_raw_fd();
}
