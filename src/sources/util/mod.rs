#[cfg(all(feature = "sources-tls", feature = "warp"))]
mod http;
#[cfg(all(feature = "sources-tls", feature = "listenfd"))]
mod tcp;
#[cfg(all(
    feature = "sources-socket",
    feature = "sources-syslog",
    feature = "unix"
))]
mod unix;

#[cfg(all(feature = "sources-tls", feature = "warp"))]
pub use self::http::{ErrorMessage, HttpSource};
#[cfg(all(feature = "sources-tls", feature = "listenfd"))]
pub use tcp::{SocketListenAddr, TcpSource};
#[cfg(all(
    feature = "sources-socket",
    feature = "sources-syslog",
    feature = "unix"
))]
pub use unix::build_unix_source;
