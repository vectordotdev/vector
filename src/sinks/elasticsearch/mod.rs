mod common;
mod config;
mod encoder;
mod request_builder;
mod retry;
mod service;
mod sink;

#[cfg(test)]
mod tests;

#[cfg(test)]
#[cfg(feature = "es-integration-tests")]
mod integration_tests;

use std::convert::TryFrom;

pub use common::*;
pub use config::*;
pub use encoder::ElasticSearchEncoder;
use http::{
    header::{HeaderName, HeaderValue},
    uri::InvalidUri,
    Request,
};
use rusoto_credential::{CredentialsError, ProvideAwsCredentials};
use rusoto_signature::SignedRequest;
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};

use crate::{
    aws::rusoto::{self, AwsAuthentication},
    config::SinkDescription,
    event::{EventRef, LogEvent},
    internal_events::TemplateRenderingFailed,
    template::{Template, TemplateParseError},
};
// use crate::sinks::elasticsearch::ParseError::AwsCredentialsGenerateFailed;

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(deny_unknown_fields, rename_all = "snake_case", tag = "strategy")]
pub enum ElasticSearchAuth {
    Basic { user: String, password: String },
    Aws(AwsAuthentication),
}

#[derive(Deserialize, Serialize, Debug, Clone, Eq, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub enum ElasticSearchMode {
    #[serde(alias = "normal")]
    Bulk,
    DataStream,
}

impl Default for ElasticSearchMode {
    fn default() -> Self {
        Self::Bulk
    }
}

#[derive(Derivative, Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub enum BulkAction {
    Index,
    Create,
}

#[allow(clippy::trivially_copy_pass_by_ref)]
impl BulkAction {
    pub const fn as_str(&self) -> &'static str {
        match self {
            BulkAction::Index => "index",
            BulkAction::Create => "create",
        }
    }

    pub const fn as_json_pointer(&self) -> &'static str {
        match self {
            BulkAction::Index => "/index",
            BulkAction::Create => "/create",
        }
    }
}

impl TryFrom<&str> for BulkAction {
    type Error = String;

    fn try_from(input: &str) -> Result<Self, Self::Error> {
        match input {
            "index" => Ok(BulkAction::Index),
            "create" => Ok(BulkAction::Create),
            _ => Err(format!("Invalid bulk action: {}", input)),
        }
    }
}

inventory::submit! {
    SinkDescription::new::<ElasticSearchConfig>("elasticsearch")
}

impl_generate_config_from_default!(ElasticSearchConfig);

#[derive(Debug, Clone)]
pub enum ElasticSearchCommonMode {
    Bulk {
        index: Template,
        action: Option<Template>,
    },
    DataStream(DataStreamConfig),
}

impl ElasticSearchCommonMode {
    fn index(&self, log: &LogEvent) -> Option<String> {
        match self {
            Self::Bulk { index, .. } => index
                .render_string(log)
                .map_err(|error| {
                    emit!(&TemplateRenderingFailed {
                        error,
                        field: Some("index"),
                        drop_event: true,
                    });
                })
                .ok(),
            Self::DataStream(ds) => ds.index(log),
        }
    }

    fn bulk_action<'a>(&self, event: impl Into<EventRef<'a>>) -> Option<BulkAction> {
        match self {
            ElasticSearchCommonMode::Bulk {
                action: bulk_action,
                ..
            } => match bulk_action {
                Some(template) => template
                    .render_string(event)
                    .map_err(|error| {
                        emit!(&TemplateRenderingFailed {
                            error,
                            field: Some("bulk_action"),
                            drop_event: true,
                        });
                    })
                    .ok()
                    .and_then(|value| BulkAction::try_from(value.as_str()).ok()),
                None => Some(BulkAction::Index),
            },
            // avoid the interpolation
            ElasticSearchCommonMode::DataStream(_) => Some(BulkAction::Create),
        }
    }

    const fn as_data_stream_config(&self) -> Option<&DataStreamConfig> {
        match self {
            Self::DataStream(value) => Some(value),
            _ => None,
        }
    }
}

#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub enum ParseError {
    #[snafu(display("Invalid host {:?}: {:?}", host, source))]
    InvalidHost { host: String, source: InvalidUri },
    #[snafu(display("Host {:?} must include hostname", host))]
    HostMustIncludeHostname { host: String },
    #[snafu(display("Could not generate AWS credentials: {:?}", source))]
    AwsCredentialsGenerateFailed { source: CredentialsError },
    #[snafu(display("Index template parse error: {}", source))]
    IndexTemplate { source: TemplateParseError },
    #[snafu(display("Batch action template parse error: {}", source))]
    BatchActionTemplate { source: TemplateParseError },
}

async fn finish_signer(
    signer: &mut SignedRequest,
    credentials_provider: &rusoto::AwsCredentialsProvider,
    mut builder: http::request::Builder,
) -> crate::Result<http::request::Builder> {
    let credentials = credentials_provider
        .credentials()
        .await
        .context(AwsCredentialsGenerateFailed)?;

    signer.sign(&credentials);

    for (name, values) in signer.headers() {
        let header_name = name
            .parse::<HeaderName>()
            .expect("Could not parse header name.");
        for value in values {
            let header_value =
                HeaderValue::from_bytes(value).expect("Could not parse header value.");
            builder = builder.header(&header_name, header_value);
        }
    }

    Ok(builder)
}
