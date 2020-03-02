#[cfg(feature = "sources-http")]
mod http;
#[cfg(feature = "sources-socket")]
mod tcp;
#[cfg(all(unix, feature = "sources-socket"))]
mod unix;

#[cfg(feature = "sources-http")]
pub use self::http::{ErrorMessage, HttpSource};
#[cfg(feature = "sources-socket")]
pub use tcp::{SocketListenAddr, TcpSource};

#[cfg(all(unix, feature = "sources-socket"))]
pub use unix::build_unix_source;
