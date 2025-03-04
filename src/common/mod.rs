//! Modules that are common between sources, transforms, and sinks.
#[cfg(any(
    feature = "sources-datadog_agent",
    feature = "sinks-datadog_events",
    feature = "sinks-datadog_logs",
    feature = "sinks-datadog_metrics",
    feature = "sinks-datadog_traces",
))]
pub mod datadog;

#[cfg(any(
    feature = "sources-aws_sqs",
    feature = "sinks-aws_sqs",
    feature = "sources-aws_s3"
))]
pub(crate) mod sqs;

#[cfg(any(feature = "sources-aws_s3", feature = "sinks-aws_s3"))]
pub(crate) mod s3;

#[cfg(any(feature = "transforms-log_to_metric", feature = "sinks-loki"))]
pub(crate) mod expansion;

#[cfg(any(
    feature = "sources-utils-http-auth",
    feature = "sources-utils-http-error"
))]
pub mod http;
