#[cfg(all(feature = "sources-tls", feature = "warp"))]
mod http;
pub mod multiline_config;
#[cfg(all(feature = "sources-tls", feature = "listenfd"))]
mod tcp;
#[cfg(all(unix, feature = "sources-utils-unix",))]
mod unix;

#[cfg(all(feature = "sources-tls", feature = "warp"))]
pub use self::http::{ErrorMessage, HttpSource, HttpSourceAuthConfig};
pub use multiline_config::MultilineConfig;
#[cfg(all(feature = "sources-tls", feature = "listenfd"))]
pub use tcp::{SocketListenAddr, TcpSource};
#[cfg(all(unix, feature = "sources-utils-unix",))]
pub use unix::build_unix_source;
