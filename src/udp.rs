#[cfg(unix)]
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd};
#[cfg(unix)]
use tokio::net::UdpSocket;

#[cfg(unix)]
pub fn set_buffer_sizes(
    socket: &mut UdpSocket,
    send_buffer_bytes: Option<usize>,
    receive_buffer_bytes: Option<usize>,
) {
    // SAFETY: We temporarily take ownership of the socket and return it by the end of this block scope.
    let socket = unsafe { socket2::Socket::from_raw_fd(socket.as_raw_fd()) };

    if let Some(send_buffer_bytes) = send_buffer_bytes {
        if let Err(error) = socket.set_send_buffer_size(send_buffer_bytes) {
            warn!(message = "Failed configuring send buffer size on UDP socket.", %error);
        }
    }

    if let Some(receive_buffer_bytes) = receive_buffer_bytes {
        if let Err(error) = socket.set_recv_buffer_size(receive_buffer_bytes) {
            warn!(message = "Failed configuring receive buffer size on UDP socket.", %error);
        }
    }

    socket.into_raw_fd();
}
