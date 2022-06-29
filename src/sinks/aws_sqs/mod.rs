mod config;
mod request_builder;
mod retry;
mod service;
mod sink;

#[cfg(feature = "aws-sqs-integration-tests")]
#[cfg(test)]
mod integration_tests;

use crate::config::SinkDescription;
pub use config::SqsSinkConfig;

inventory::submit! {
    SinkDescription::new::<config::SqsSinkConfig>("aws_sqs")
}
