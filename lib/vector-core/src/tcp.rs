use socket2::SockRef;
use tokio::net::TcpStream;
use vector_config::configurable_component;

/// TCP keepalive settings for socket-based components.
#[configurable_component]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
#[configurable(metadata(docs::human_name = "Wait Time"))]
pub struct TcpKeepaliveConfig {
    /// The time to wait before starting to send TCP keepalive probes on an idle connection.
    #[configurable(metadata(docs::type_unit = "seconds"))]
    pub time_secs: Option<u64>,
}

// This function will be obsolete after tokio/mio internally use `socket2` and expose the methods to
// apply options to a socket.
pub(crate) fn set_keepalive(
    socket: &TcpStream,
    params: &socket2::TcpKeepalive,
) -> std::io::Result<()> {
    SockRef::from(socket).set_tcp_keepalive(params)
}

// This function will be obsolete after tokio/mio internally use `socket2` and expose the methods to
// apply options to a socket.
pub(crate) fn set_receive_buffer_size(socket: &TcpStream, size: usize) -> std::io::Result<()> {
    SockRef::from(socket).set_recv_buffer_size(size)
}

// This function will be obsolete after tokio/mio internally use `socket2` and expose the methods to
// apply options to a socket.
pub(crate) fn set_send_buffer_size(socket: &TcpStream, size: usize) -> std::io::Result<()> {
    SockRef::from(socket).set_send_buffer_size(size)
}
