#[cfg(feature = "sources-http")]
mod http;
#[cfg(feature = "sources-tls")]
mod tcp;
#[cfg(unix)]
mod unix;

#[cfg(feature = "sources-http")]
pub use self::http::{ErrorMessage, HttpSource};
#[cfg(feature = "sources-tls")]
pub use tcp::{SocketListenAddr, TcpSource};
#[cfg(unix)]
pub use unix::build_unix_source;
