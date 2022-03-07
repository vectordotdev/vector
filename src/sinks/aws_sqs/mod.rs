mod config;
mod request_builder;
mod retry;
mod service;
mod sink;

#[cfg(feature = "aws-sqs-integration-tests")]
#[cfg(test)]
mod integration_tests;
#[cfg(test)]
mod tests;

use crate::config::SinkDescription;

inventory::submit! {
    SinkDescription::new::<config::SqsSinkConfig>("aws_sqs")
}
