mod config;
mod integration_tests;
mod request_builder;
mod service;
mod sink;
mod tests;

use config::KinesisFirehoseSinkConfig;

use crate::config::SinkDescription;

inventory::submit! {
    SinkDescription::new::<KinesisFirehoseSinkConfig>("aws_kinesis_firehose")
}
