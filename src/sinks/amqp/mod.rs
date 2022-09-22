//! `AMQP` sink.
//! Handles version AMQP 0.9.1 which is used by RabbitMQ.
mod config;
mod encoder;
mod request_builder;
mod service;
mod sink;

#[cfg(all(test, feature = "amqp-integration-tests"))]
mod integration_tests;

pub use config::AmqpSinkConfig;
use snafu::Snafu;

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("creating amqp producer failed: {}", source))]
    AmqpCreateFailed {
        source: Box<dyn std::error::Error + Send + Sync>,
    },
}
