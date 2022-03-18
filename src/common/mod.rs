#[cfg(any(feature = "sources-datadog_agent", feature = "sinks-datadog_metrics"))]
pub(crate) mod datadog;

#[cfg(any(feature = "sources-aws_sqs", feature = "sinks-aws_sqs"))]
pub(crate) mod sqs;

#[cfg(any(feature = "sinks-aws_s3"))]
pub(crate) mod s3;
