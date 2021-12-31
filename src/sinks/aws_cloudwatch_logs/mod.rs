mod config;
mod healthcheck;
mod request;
mod request_builder;
mod retry;
mod service;
mod sink;

mod integration_tests;

use snafu::Snafu;

use self::config::CloudwatchLogsSinkConfig;
use crate::{config::SinkDescription, internal_events::TemplateRenderingFailed};

#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub enum CloudwatchLogsError {
    #[snafu(display("{}", source))]
    HttpClientError {
        source: rusoto_core::request::TlsError,
    },
    #[snafu(display("{}", source))]
    InvalidCloudwatchCredentials {
        source: rusoto_credential::CredentialsError,
    },
    #[snafu(display("Encoded event is too long, length={}", length))]
    EventTooLong { length: usize },

    #[snafu(display("{}", source))]
    IoError { source: std::io::Error },
}

inventory::submit! {
    SinkDescription::new::<CloudwatchLogsSinkConfig>("aws_cloudwatch_logs")
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct CloudwatchKey {
    group: String,
    stream: String,
}
