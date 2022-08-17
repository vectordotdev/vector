mod config;
mod encoder;
mod request_builder;
mod service;
mod sink;

#[cfg(test)]
mod tests;

use config::AMQPSinkConfig;
use snafu::Snafu;

use crate::{config::SinkDescription, template::TemplateParseError};

inventory::submit! {
    SinkDescription::new::<AMQPSinkConfig>("amqp")
}

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("creating amqp producer failed: {}", source))]
    AMQPCreateFailed {
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    #[snafu(display("invalid exchange template: {}", source))]
    ExchangeTemplate { source: TemplateParseError },
    #[snafu(display("invalid routing key template: {}", source))]
    RoutingKeyTemplate { source: TemplateParseError },
}
