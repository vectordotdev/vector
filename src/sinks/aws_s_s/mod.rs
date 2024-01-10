mod client;
mod config;
mod request_builder;
mod retry;
mod service;
mod sink;

#[cfg(feature = "sinks-aws_sqs")]
mod sqs;

#[cfg(feature = "sinks-aws_sns")]
mod sns;
