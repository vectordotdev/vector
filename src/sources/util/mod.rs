#[cfg(any(feature = "sources-http"))]
mod body_decoding;
#[cfg(any(
    all(feature = "sources-utils-tls", feature = "listenfd"),
    feature = "codecs",
))]
mod codecs;
mod encoding_config;
#[cfg(any(
    feature = "sources-file",
    feature = "sources-kafka",
    feature = "sources-splunk_hec"
))]
pub mod finalizer;
#[cfg(all(unix, feature = "sources-dnstap"))]
pub mod framestream;
#[cfg(any(
    feature = "sources-utils-http-auth",
    feature = "sources-utils-http-encoding",
    feature = "sources-utils-http-error",
    feature = "sources-utils-http-prelude",
    feature = "sources-utils-http-query"
))]
mod http;
pub mod multiline_config;
#[cfg(all(feature = "sources-utils-tls", feature = "listenfd"))]
mod tcp;
#[cfg(all(unix, feature = "sources-socket"))]
mod unix_datagram;
#[cfg(all(unix, feature = "sources-utils-unix"))]
mod unix_stream;
#[cfg(any(feature = "sources-utils-tls", feature = "sources-vector"))]
mod wrappers;

#[cfg(any(
    all(feature = "sources-utils-tls", feature = "listenfd"),
    feature = "codecs",
))]
pub use codecs::StreamDecodingError;
pub use encoding_config::EncodingConfig;
pub use multiline_config::MultilineConfig;
#[cfg(all(feature = "sources-utils-tls", feature = "listenfd"))]
pub use tcp::{SocketListenAddr, TcpNullAcker, TcpSource, TcpSourceAck, TcpSourceAcker};
#[cfg(all(unix, feature = "sources-socket",))]
pub use unix_datagram::build_unix_datagram_source;
#[cfg(all(unix, feature = "sources-utils-unix",))]
pub use unix_stream::build_unix_stream_source;
#[cfg(any(feature = "sources-utils-tls", feature = "sources-vector"))]
pub use wrappers::AfterReadExt;

#[cfg(any(feature = "sources-http"))]
pub use self::body_decoding::Encoding;
#[cfg(feature = "sources-utils-http-query")]
pub use self::http::add_query_parameters;
#[cfg(any(
    feature = "sources-prometheus",
    feature = "sources-utils-http-encoding"
))]
pub use self::http::decode;
#[cfg(feature = "sources-utils-http-error")]
pub use self::http::ErrorMessage;
#[cfg(feature = "sources-utils-http-prelude")]
pub use self::http::HttpSource;
#[cfg(feature = "sources-utils-http-auth")]
pub use self::http::HttpSourceAuthConfig;
