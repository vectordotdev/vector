#[cfg(feature = "sources-http")]
mod http;
mod tcp;
#[cfg(unix)]
mod unix;

#[cfg(feature = "sources-http")]
pub use self::http::{ErrorMessage, HttpSource};
pub use tcp::{SocketListenAddr, TcpSource};
#[cfg(unix)]
pub use unix::build_unix_source;
