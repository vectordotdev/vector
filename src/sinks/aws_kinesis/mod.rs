#[cfg(feature = "sinks-aws_kinesis_streams")]
pub mod streams;

#[cfg(feature = "sinks-aws_kinesis_firehose")]
pub mod firehose;

pub mod config;
pub mod record;
pub mod request_builder;
pub mod service;
pub mod sink;

pub use service::KinesisResponse;
pub use service::KinesisService;
