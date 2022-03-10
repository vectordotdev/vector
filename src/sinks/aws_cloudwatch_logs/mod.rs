mod config;
mod healthcheck;
mod request;
mod request_builder;
mod retry;
mod service;
mod sink;

mod integration_tests;

use self::config::CloudwatchLogsSinkConfig;
use crate::{config::SinkDescription, internal_events::TemplateRenderingError};

inventory::submit! {
    SinkDescription::new::<CloudwatchLogsSinkConfig>("aws_cloudwatch_logs")
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct CloudwatchKey {
    group: String,
    stream: String,
}
