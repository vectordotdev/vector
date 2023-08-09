mod config;
mod request_builder;
mod retry;
mod service;
mod sink;

pub use self::config::SqsSinkConfig;

#[cfg(feature = "sinks-aws_sqs")]
pub mod sqs;
