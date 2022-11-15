#[cfg(any(feature = "sources-http_server"))]
mod body_decoding;
mod encoding_config;
#[cfg(all(unix, feature = "sources-dnstap"))]
pub mod framestream;
#[cfg(any(feature = "sources-vector", feature = "sources-opentelemetry"))]
pub mod grpc;
#[cfg(any(
    feature = "sources-utils-http-auth",
    feature = "sources-utils-http-encoding",
    feature = "sources-utils-http-error",
    feature = "sources-utils-http-prelude",
    feature = "sources-utils-http-query"
))]
pub mod http;
#[cfg(any(feature = "sources-http_client", feature = "sources-prometheus"))]
pub mod http_client;
#[cfg(any(feature = "sources-aws_sqs", feature = "sources-gcp_pubsub"))]
mod message_decoding;
pub mod multiline_config;
#[cfg(any(feature = "sources-utils-net-tcp", feature = "sources-utils-net-udp"))]
pub mod net;
#[cfg(all(
    unix,
    any(feature = "sources-socket", feature = "sources-utils-net-unix",)
))]
pub mod unix;
#[cfg(all(unix, feature = "sources-socket"))]
mod unix_datagram;
#[cfg(all(unix, feature = "sources-utils-net-unix"))]
mod unix_stream;
mod wrappers;

#[cfg(feature = "sources-file")]
pub use encoding_config::EncodingConfig;
pub use multiline_config::MultilineConfig;
#[cfg(all(
    unix,
    any(feature = "sources-socket", feature = "sources-utils-net-unix",)
))]
pub use unix::change_socket_permissions;
#[cfg(all(unix, feature = "sources-socket",))]
pub use unix_datagram::build_unix_datagram_source;
#[cfg(all(unix, feature = "sources-utils-net-unix",))]
pub use unix_stream::build_unix_stream_source;
pub use wrappers::{AfterRead, AfterReadExt};

#[cfg(any(feature = "sources-http_server"))]
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
#[cfg(any(feature = "sources-aws_sqs", feature = "sources-gcp_pubsub"))]
pub use self::message_decoding::decode_message;
