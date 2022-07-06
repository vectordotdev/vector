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
pub use encoder::ElasticsearchEncoder;
use http::{uri::InvalidUri, Request};
use serde::{Deserialize, Serialize};
use snafu::Snafu;

use crate::aws::AwsAuthentication;
use crate::{
    config::SinkDescription,
    event::{EventRef, LogEvent},
    internal_events::TemplateRenderingError,
    template::{Template, TemplateParseError},
};

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(deny_unknown_fields, rename_all = "snake_case", tag = "strategy")]
pub enum ElasticsearchAuth {
    Basic { user: String, password: String },
    Aws(AwsAuthentication),
}

#[derive(Deserialize, Serialize, Debug, Clone, Eq, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub enum ElasticsearchMode {
    #[serde(alias = "normal")]
    Bulk,
    DataStream,
}

impl Default for ElasticsearchMode {
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
    SinkDescription::new::<ElasticsearchConfig>("elasticsearch")
}

impl_generate_config_from_default!(ElasticsearchConfig);

#[derive(Debug, Clone)]
pub enum ElasticsearchCommonMode {
    Bulk {
        index: Template,
        action: Option<Template>,
    },
    DataStream(DataStreamConfig),
}

impl ElasticsearchCommonMode {
    fn index(&self, log: &LogEvent) -> Option<String> {
        match self {
            Self::Bulk { index, .. } => index
                .render_string(log)
                .map_err(|error| {
                    emit!(TemplateRenderingError {
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
            ElasticsearchCommonMode::Bulk {
                action: bulk_action,
                ..
            } => match bulk_action {
                Some(template) => template
                    .render_string(event)
                    .map_err(|error| {
                        emit!(TemplateRenderingError {
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
            ElasticsearchCommonMode::DataStream(_) => Some(BulkAction::Create),
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
    #[snafu(display("Index template parse error: {}", source))]
    IndexTemplate { source: TemplateParseError },
    #[snafu(display("Batch action template parse error: {}", source))]
    BatchActionTemplate { source: TemplateParseError },
    #[snafu(display("aws.region required when AWS authentication is in use"))]
    RegionRequired,
}
