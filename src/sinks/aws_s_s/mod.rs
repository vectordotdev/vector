pub mod config;
mod request_builder;
mod retry;
mod service;
mod sink;

pub use self::config::BaseSSSinkConfig;

#[cfg(feature = "sinks-aws_sqs")]
pub mod sqs;

#[cfg(feature = "sinks-aws_sns")]
pub mod sns;

mod client;
