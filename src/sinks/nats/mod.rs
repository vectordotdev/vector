//! `NATS` sink
//! Publishes data using [NATS](nats.io)(Neural Autonomic Transport System).

use snafu::Snafu;

use crate::nats::NatsConfigError;

mod config;
#[cfg(feature = "nats-integration-tests")]
#[cfg(test)]
mod integration_tests;
mod request_builder;
mod service;
mod sink;
#[cfg(test)]
mod tests;

#[derive(Debug, Snafu)]
enum NatsError {
    #[snafu(display("invalid encoding: {}", source))]
    Encoding {
        source: vector_lib::codecs::encoding::BuildError,
    },
    #[snafu(display("NATS Config Error: {}", source))]
    Config { source: NatsConfigError },
    #[snafu(display("NATS Connect Error: {}", source))]
    Connect { source: async_nats::ConnectError },
    #[snafu(display("NATS Server Error: {}", source))]
    ServerError { source: async_nats::Error },
}
