#[cfg(any(
    feature = "sources-datadog_agent",
    feature = "sinks-datadog_archives",
    feature = "sinks-datadog_events",
    feature = "sinks-datadog_logs",
    feature = "sinks-datadog_metrics",
    feature = "enterprise"
))]
pub(crate) mod datadog;

#[cfg(any(
    feature = "sources-aws_sqs",
    feature = "sinks-aws_sqs",
    feature = "sources-aws_s3"
))]
pub(crate) mod sqs;

#[cfg(any(feature = "sinks-aws_s3"))]
pub(crate) mod s3;
