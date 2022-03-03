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
use crate::{config::SinkDescription, internal_events::TemplateRenderingError};

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
}

inventory::submit! {
    SinkDescription::new::<CloudwatchLogsSinkConfig>("aws_cloudwatch_logs")
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct CloudwatchKey {
    group: String,
    stream: String,
}
