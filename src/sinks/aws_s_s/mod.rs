pub mod config;
mod request_builder;
mod retry;
mod service;
mod sink;

use self::config::BaseSSSinkConfig;

#[cfg(feature = "sinks-aws_sqs")]
mod sqs;

#[cfg(feature = "sinks-aws_sns")]
mod sns;

mod client;
