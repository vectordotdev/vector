//! Modules that are common between sources and sinks.
#[cfg(any(
    feature = "sources-datadog_agent",
    feature = "sinks-datadog_events",
    feature = "sinks-datadog_logs",
    feature = "sinks-datadog_metrics",
    feature = "sinks-datadog_traces",
    feature = "enterprise"
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
