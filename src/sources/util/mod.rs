#[cfg(all(feature = "sources-tls", feature = "warp"))]
mod http;
#[cfg(all(feature = "sources-tls", feature = "listenfd"))]
mod tcp;
#[cfg(unix)]
mod unix;

#[cfg(all(feature = "sources-tls", feature = "warp"))]
pub use self::http::{ErrorMessage, HttpSource};
#[cfg(all(feature = "sources-tls", feature = "listenfd"))]
pub use tcp::{SocketListenAddr, TcpSource};
#[cfg(unix)]
pub use unix::build_unix_source;
