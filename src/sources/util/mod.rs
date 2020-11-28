#[cfg(feature = "sources-utils-http")]
mod http;
pub mod multiline_config;
#[cfg(all(feature = "sources-utils-tls", feature = "listenfd"))]
mod tcp;
#[cfg(all(unix, feature = "sources-utils-unix",))]
mod unix;

#[cfg(any(feature = "sources-http", feature = "sources-logplex"))]
pub(crate) use self::http::add_query_parameters;
#[cfg(feature = "sources-utils-http")]
pub(crate) use self::http::{ErrorMessage, HttpSource, HttpSourceAuthConfig};
pub use multiline_config::MultilineConfig;
#[cfg(all(feature = "sources-utils-tls", feature = "listenfd"))]
pub use tcp::{SocketListenAddr, TcpSource};
#[cfg(all(unix, feature = "sources-utils-unix",))]
pub use unix::build_unix_source;
