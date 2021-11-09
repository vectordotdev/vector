mod config;
mod integration_tests;
mod request_builder;
mod service;
mod sink;
mod tests;

use crate::config::SinkDescription;
use config::KinesisFirehoseSinkConfig;

inventory::submit! {
    SinkDescription::new::<KinesisFirehoseSinkConfig>("aws_kinesis_firehose")
}
