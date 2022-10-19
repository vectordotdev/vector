#[cfg(feature = "sinks-aws_kinesis_streams")]
pub mod streams;

#[cfg(feature = "sinks-aws_kinesis_firehose")]
pub mod firehose;

pub mod request_builder;
pub mod sink;
