#[cfg(any(feature = "sources-http"))]
mod body_decoding;
mod encoding_config;
#[cfg(any(
    feature = "sources-aws_sqs",
    feature = "sources-file",
    feature = "sources-gcp_pubsub",
    feature = "sources-journald",
    feature = "sources-kafka",
    feature = "sources-splunk_hec"
))]
pub mod finalizer;
#[cfg(all(unix, feature = "sources-dnstap"))]
pub mod framestream;
#[cfg(feature = "sources-vector")]
pub mod grpc;
#[cfg(any(
    feature = "sources-utils-http-auth",
    feature = "sources-utils-http-encoding",
    feature = "sources-utils-http-error",
    feature = "sources-utils-http-prelude",
    feature = "sources-utils-http-query"
))]
mod http;
#[cfg(any(feature = "sources-aws_sqs", feature = "sources-gcp_pubsub"))]
mod message_decoding;
pub mod multiline_config;
#[cfg(all(feature = "sources-utils-tls", feature = "listenfd"))]
mod tcp;
#[cfg(all(unix, any(feature = "sources-socket", feature = "sources-utils-unix",)))]
mod unix;
#[cfg(all(unix, feature = "sources-socket"))]
mod unix_datagram;
#[cfg(all(unix, feature = "sources-utils-unix"))]
mod unix_stream;
#[cfg(any(
    feature = "sources-utils-tls",
    feature = "sources-vector",
    feature = "sources-gcp_pubsub"
))]
mod wrappers;

#[cfg(feature = "sources-file")]
pub use encoding_config::EncodingConfig;
pub use multiline_config::MultilineConfig;
#[cfg(all(feature = "sources-utils-tls", feature = "listenfd"))]
pub use tcp::{SocketListenAddr, TcpNullAcker, TcpSource, TcpSourceAck, TcpSourceAcker};
#[cfg(all(unix, any(feature = "sources-socket", feature = "sources-utils-unix",)))]
pub use unix::change_socket_permissions;
#[cfg(all(unix, feature = "sources-socket",))]
pub use unix_datagram::build_unix_datagram_source;
#[cfg(all(unix, feature = "sources-utils-unix",))]
pub use unix_stream::build_unix_stream_source;
#[cfg(any(
    feature = "sources-utils-tls",
    feature = "sources-vector",
    feature = "sources-gcp_pubsub"
))]
pub use wrappers::{AfterRead, AfterReadExt};

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
#[cfg(any(feature = "sources-aws_sqs", feature = "sources-gcp_pubsub"))]
pub use self::message_decoding::decode_message;
