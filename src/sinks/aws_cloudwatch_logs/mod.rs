mod config;
mod healthcheck;
mod request;
mod request_builder;
mod retry;
mod service;
mod sink;

#[cfg(all(test, feature = "aws-cloudwatch-logs-integration-tests"))]
mod integration_tests;

pub use self::config::CloudwatchLogsSinkConfig;
use crate::internal_events::TemplateRenderingError;

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct CloudwatchKey {
    group: String,
    stream: String,
}
