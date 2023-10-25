#![allow(missing_docs)]
#[cfg(feature = "sources-http_server")]
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
#[cfg(any(feature = "sources-http_client", feature = "sources-prometheus-scrape",))]
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

#[cfg(feature = "sources-http_server")]
pub use self::body_decoding::Encoding;
#[cfg(feature = "sources-utils-http-query")]
pub use self::http::add_query_parameters;
#[cfg(any(
    feature = "sources-prometheus-scrape",
    feature = "sources-prometheus-remote-write",
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

/// Extract a tag and it's value from input string delimited by a colon character.
///
/// Note: the behavior of StatsD if more than one colon is found (which would presumably
/// be part of the tag value), is to remove any additional colons from the tag value.
/// Thus Vector expects only one colon character to be present per chunk, so the find()
/// operation locating the first position is sufficient.
#[cfg(any(feature = "sources-statsd", feature = "sources-datadog_agent"))]
pub fn extract_tag_key_and_value<S: AsRef<str>>(
    tag_chunk: S,
) -> (String, vector_lib::event::metric::TagValue) {
    use vector_lib::event::metric::TagValue;

    let tag_chunk = tag_chunk.as_ref();

    // tag_chunk is expected to be formatted as "tag_name:tag_value"
    // If no colon is found, then it is classified as a Bare tag.
    match tag_chunk.split_once(':') {
        // the notation `tag:` is valid for StatsD. The effect is an empty string value.
        Some((prefix, suffix)) => (prefix.to_string(), TagValue::Value(suffix.to_string())),
        None => (tag_chunk.to_string(), TagValue::Bare),
    }
}
