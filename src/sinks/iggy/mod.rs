//! `Iggy` sink
//! Publishes data to a topic on the [Iggy](https://iggy.apache.org) message
//! streaming platform.

use snafu::Snafu;

mod config;
#[cfg(feature = "iggy-integration-tests")]
#[cfg(test)]
mod integration_tests;
mod request_builder;
mod service;
mod sink;
#[cfg(test)]
mod tests;

#[derive(Debug, Snafu)]
enum IggyError {
    #[snafu(display("invalid encoding: {}", source))]
    Encoding {
        source: vector_lib::codecs::encoding::BuildError,
    },
    #[snafu(display("invalid batch settings"))]
    InvalidBatchSettings,
    #[snafu(display("Iggy connection error: {}", source))]
    Connect { source: iggy::prelude::IggyError },
    #[snafu(display("Iggy producer error: {}", source))]
    Producer { source: iggy::prelude::IggyError },
}
