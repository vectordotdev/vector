mod common;
mod config;
pub mod encoder;
pub mod health;
pub mod request_builder;
pub mod retry;
pub mod service;
pub mod sink;

#[cfg(test)]
mod tests;

#[cfg(test)]
#[cfg(feature = "es-integration-tests")]
mod integration_tests;

use std::{convert::TryFrom, fmt};

pub use common::*;
pub use config::*;
pub use encoder::ElasticsearchEncoder;
use http::{uri::InvalidUri, Request};
use snafu::Snafu;
use vector_lib::sensitive_string::SensitiveString;
use vector_lib::{configurable::configurable_component, internal_event};

use crate::{
    event::{EventRef, LogEvent},
    internal_events::TemplateRenderingError,
    template::{Template, TemplateParseError},
};

/// Elasticsearch Authentication strategies.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields, rename_all = "snake_case", tag = "strategy")]
#[configurable(metadata(docs::enum_tag_description = "The authentication strategy to use."))]
pub enum ElasticsearchAuthConfig {
    /// HTTP Basic Authentication.
    Basic {
        /// Basic authentication username.
        #[configurable(metadata(docs::examples = "${ELASTICSEARCH_USERNAME}"))]
        #[configurable(metadata(docs::examples = "username"))]
        user: String,

        /// Basic authentication password.
        #[configurable(metadata(docs::examples = "${ELASTICSEARCH_PASSWORD}"))]
        #[configurable(metadata(docs::examples = "password"))]
        password: SensitiveString,
    },

    #[cfg(feature = "aws-core")]
    /// Amazon OpenSearch Service-specific authentication.
    Aws(crate::aws::AwsAuthentication),
}

/// Elasticsearch Indexing mode.
#[configurable_component]
#[derive(Clone, Debug, Eq, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub enum ElasticsearchMode {
    /// Ingests documents in bulk, using the bulk API `index` action.
    #[serde(alias = "normal")]
    Bulk,

    /// Ingests documents in bulk, using the bulk API `create` action.
    ///
    /// Elasticsearch Data Streams only support the `create` action.
    DataStream,
}

impl Default for ElasticsearchMode {
    fn default() -> Self {
        Self::Bulk
    }
}

/// Bulk API actions.
#[configurable_component]
#[derive(Clone, Copy, Debug, Derivative, Eq, Hash, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub enum BulkAction {
    /// The `index` action.
    Index,

    /// The `create` action.
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

/// Elasticsearch version types.
#[configurable_component]
#[derive(Clone, Copy, Debug, Derivative, Eq, Hash, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub enum VersionType {
    /// The `internal` type.
    Internal,

    /// The `external` or `external_gt` type.
    External,

    /// The `external_gte` type.
    ExternalGte,
}

#[allow(clippy::trivially_copy_pass_by_ref)]
impl VersionType {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Internal => "internal",
            Self::External => "external",
            Self::ExternalGte => "external_gte",
        }
    }
}

impl TryFrom<&str> for VersionType {
    type Error = String;

    fn try_from(input: &str) -> Result<Self, Self::Error> {
        match input {
            "internal" => Ok(VersionType::Internal),
            "external" | "external_gt" => Ok(VersionType::External),
            "external_gte" => Ok(VersionType::ExternalGte),
            _ => Err(format!("Invalid versioning mode: {}", input)),
        }
    }
}

impl_generate_config_from_default!(ElasticsearchConfig);

#[derive(Debug, Clone)]
pub enum ElasticsearchCommonMode {
    Bulk {
        index: Template,
        action: Template,
        version: Option<Template>,
        version_type: VersionType,
    },
    DataStream(DataStreamConfig),
}

struct VersionValueParseError<'a> {
    value: &'a str,
}

impl internal_event::InternalEvent for VersionValueParseError<'_> {
    fn emit(self) {
        warn!("{self}")
    }
}

impl fmt::Display for VersionValueParseError<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Cannot parse version \"{}\" as integer", self.value)
    }
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
                action: bulk_action_template,
                ..
            } => bulk_action_template
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
            // avoid the interpolation
            ElasticsearchCommonMode::DataStream(_) => Some(BulkAction::Create),
        }
    }

    fn version<'a>(&self, event: impl Into<EventRef<'a>>) -> Option<u64> {
        match self {
            ElasticsearchCommonMode::Bulk {
                version: Some(version),
                ..
            } => version
                .render_string(event)
                .map_err(|error| {
                    emit!(TemplateRenderingError {
                        error,
                        field: Some("version"),
                        drop_event: true,
                    });
                })
                .ok()
                .as_ref()
                .and_then(|value| {
                    value
                        .parse()
                        .map_err(|_| emit!(VersionValueParseError { value }))
                        .ok()
                }),
            _ => None,
        }
    }

    const fn version_type(&self) -> Option<VersionType> {
        match self {
            ElasticsearchCommonMode::Bulk { version_type, .. } => Some(*version_type),
            _ => Some(VersionType::Internal),
        }
    }

    const fn as_data_stream_config(&self) -> Option<&DataStreamConfig> {
        match self {
            Self::DataStream(value) => Some(value),
            _ => None,
        }
    }
}

/// Configuration for Elasticsearch API version.
#[configurable_component]
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub enum ElasticsearchApiVersion {
    /// Auto-detect the API version.
    ///
    /// If the [cluster state version endpoint][es_version] isn't reachable, a warning is logged to
    /// stdout, and the version is assumed to be V6 if the `suppress_type_name` option is set to
    /// `true`. Otherwise, the version is assumed to be V8. In the future, the sink instead
    /// returns an error during configuration parsing, since a wrongly assumed version could lead to
    /// incorrect API calls.
    ///
    /// [es_version]: https://www.elastic.co/guide/en/elasticsearch/reference/current/cluster-state.html#cluster-state-api-path-params
    Auto,
    /// Use the Elasticsearch 6.x API.
    V6,
    /// Use the Elasticsearch 7.x API.
    V7,
    /// Use the Elasticsearch 8.x API.
    V8,
}

impl Default for ElasticsearchApiVersion {
    fn default() -> Self {
        Self::Auto
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
    #[cfg(feature = "aws-core")]
    #[snafu(display("aws.region required when AWS authentication is in use"))]
    RegionRequired,
    #[snafu(display("Endpoints option must be specified"))]
    EndpointRequired,
    #[snafu(display(
        "`endpoint` and `endpoints` options are mutually exclusive. Please use `endpoints` option."
    ))]
    EndpointsExclusive,
    #[snafu(display("Tried to use external versioning without specifying the version itself"))]
    ExternalVersioningWithoutVersion,
    #[snafu(display("Cannot use external versioning without specifying a document ID"))]
    ExternalVersioningWithoutDocumentID,
    #[snafu(display("Your version field will be ignored because you use internal versioning"))]
    ExternalVersionIgnoredWithInternalVersioning,
}
