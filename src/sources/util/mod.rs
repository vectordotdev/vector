pub mod fake;

#[cfg(feature = "sources-utils-http")]
mod http;
pub mod multiline_config;
#[cfg(all(feature = "tls", feature = "listenfd"))]
mod tcp;
#[cfg(all(unix, feature = "sources-utils-unix",))]
mod unix;

#[cfg(feature = "sources-utils-http")]
pub use self::http::{add_query_parameters, ErrorMessage, HttpSource, HttpSourceAuthConfig};
pub use multiline_config::MultilineConfig;
#[cfg(all(feature = "tls", feature = "listenfd"))]
pub use tcp::{SocketListenAddr, TcpSource};
#[cfg(all(unix, feature = "sources-utils-unix",))]
pub use unix::build_unix_source;
