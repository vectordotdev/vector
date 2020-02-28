#![cfg(feature = "sources-socket")]
mod tcp;
#[cfg(all(unix))]
mod unix;

pub use tcp::{SocketListenAddr, TcpSource};
#[cfg(all(unix))]
pub use unix::build_unix_source;
