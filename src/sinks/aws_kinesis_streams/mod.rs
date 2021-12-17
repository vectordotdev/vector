mod config;
mod integration_tests;
mod request_builder;
mod service;
mod sink;

use config::KinesisSinkConfig;

use crate::config::SinkDescription;

inventory::submit! {
    SinkDescription::new::<KinesisSinkConfig>("aws_kinesis_streams")
}
