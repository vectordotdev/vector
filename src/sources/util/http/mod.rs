#[cfg(feature = "sources-utils-http-encoding")]
mod encoding;
#[cfg(any(
    feature = "sources-http_server",
    feature = "sources-opentelemetry",
    feature = "sources-utils-http-headers"
))]
mod headers;
mod method;
#[cfg(feature = "sources-utils-http-prelude")]
mod prelude;
#[cfg(any(
    feature = "sources-http_server",
    feature = "sources-heroku_logs",
    feature = "sources-utils-http-query"
))]
mod query;

#[cfg(any(
    feature = "sources-aws_kinesis_firehose",
    feature = "sources-datadog_agent",
    feature = "sources-opentelemetry",
    feature = "sources-splunk_hec",
    all(test, feature = "sources-http_client"),
))]
pub(crate) use encoding::capped_body;
#[cfg(feature = "sources-utils-http-encoding")]
pub use encoding::{decompress_body, emit_decompress_error, set_max_decompressed_size_bytes};
#[cfg(feature = "sources-utils-http-headers")]
pub use headers::add_headers;
pub use method::HttpMethod;
#[cfg(feature = "sources-utils-http-prelude")]
pub use prelude::HttpSource;
#[cfg(feature = "sources-utils-http-query")]
pub use query::add_query_parameters;
