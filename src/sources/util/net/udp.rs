use std::io;

use listenfd::ListenFd;
use tokio::net::UdpSocket;

use super::SocketListenAddr;

/// Binds a UDP socket to the listen address.
pub async fn try_bind_udp_socket(
    addr: SocketListenAddr,
    mut listenfd: ListenFd,
) -> io::Result<UdpSocket> {
    match addr {
        SocketListenAddr::SocketAddr(addr) => UdpSocket::bind(&addr).await,
        SocketListenAddr::SystemdFd(offset) => match listenfd.take_udp_socket(offset)? {
            Some(socket) => UdpSocket::from_std(socket),
            None => Err(io::Error::new(
                io::ErrorKind::AddrInUse,
                "systemd fd already consumed",
            )),
        },
    }
}
