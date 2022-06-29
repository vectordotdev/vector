mod config;
mod integration_tests;
mod request_builder;
mod service;
mod sink;
mod tests;

pub use self::config::KinesisFirehoseSinkConfig;

use crate::config::SinkDescription;

inventory::submit! {
    SinkDescription::new::<KinesisFirehoseSinkConfig>("aws_kinesis_firehose")
}
