mod integration_tests;
mod tests;
mod config;
mod service;
mod sink;
mod request_builder;


use crate::config::SinkDescription;
use config::KinesisFirehoseSinkConfig;

inventory::submit! {
    SinkDescription::new::<KinesisFirehoseSinkConfig>("aws_kinesis_firehose")
}
