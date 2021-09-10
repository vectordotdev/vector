#[cfg(any(feature = "sources-http"))]
mod body_decoding;
mod encoding_config;
#[cfg(any(feature = "sources-file", feature = "sources-kafka"))]
pub mod finalizer;
#[cfg(all(unix, feature = "sources-dnstap"))]
pub mod framestream;
#[cfg(feature = "sources-utils-http")]
mod http;
pub mod multiline_config;
#[cfg(all(feature = "sources-utils-tls", feature = "listenfd"))]
mod tcp;
#[cfg(any(
    all(feature = "sources-utils-tls", feature = "listenfd"),
    feature = "codecs",
))]
mod tcp_error;
#[cfg(all(unix, feature = "sources-socket"))]
mod unix_datagram;
#[cfg(all(unix, feature = "sources-utils-unix"))]
mod unix_stream;

#[cfg(any(feature = "sources-http"))]
pub use self::body_decoding::{decode_body, Encoding};
#[cfg(any(feature = "sources-http", feature = "sources-heroku_logs"))]
pub use self::http::add_query_parameters;
#[cfg(feature = "sources-prometheus")]
pub use self::http::decode;
#[cfg(feature = "sources-utils-http")]
pub use self::http::{ErrorMessage, HttpSource, HttpSourceAuthConfig};
pub use encoding_config::EncodingConfig;
pub use multiline_config::MultilineConfig;
#[cfg(all(feature = "sources-utils-tls", feature = "listenfd"))]
pub use tcp::{SocketListenAddr, TcpSource};
#[cfg(any(
    all(feature = "sources-utils-tls", feature = "listenfd"),
    feature = "codecs",
))]
pub use tcp_error::TcpError;
#[cfg(all(unix, feature = "sources-socket",))]
pub use unix_datagram::build_unix_datagram_source;
#[cfg(all(unix, feature = "sources-utils-unix",))]
pub use unix_stream::build_unix_stream_source;
