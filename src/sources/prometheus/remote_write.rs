use std::{collections::HashMap, net::SocketAddr};

use bytes::Bytes;
use prost::Message;
use vector_lib::{
    config::LogNamespace, configurable::configurable_component, prometheus::parser::proto,
};
use warp::http::{HeaderMap, StatusCode};

use super::parser;

use crate::{
    common::http::{ErrorMessage, server_auth::HttpServerAuthConfig},
    config::{
        GenerateConfig, SourceAcknowledgementsConfig, SourceConfig, SourceContext, SourceOutput,
    },
    event::Event,
    http::KeepaliveConfig,
    internal_events::PrometheusRemoteWriteParseError,
    serde::bool_or_struct,
    sources::{
        self,
        util::{HttpSource, decompress_body, http::HttpMethod},
    },
    tls::TlsEnableableConfig,
};

/// Defines the behavior for handling conflicting metric metadata.
#[configurable_component]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MetadataConflictStrategy {
    /// Silently ignore metadata conflicts, keeping the first metadata entry. This aligns with Prometheus/Thanos behavior.
    Ignore,
    /// Reject requests with conflicting metadata by returning an HTTP 400 error. This is the default to preserve backwards compatibility.
    #[default]
    Reject,
}

/// Configuration for the `prometheus_remote_write` source.
#[configurable_component(source(
    "prometheus_remote_write",
    "Receive metric via the Prometheus Remote Write protocol."
))]
#[derive(Clone, Debug)]
pub struct PrometheusRemoteWriteConfig {
    /// The socket address to accept connections on.
    ///
    /// The address _must_ include a port.
    #[configurable(metadata(docs::examples = "0.0.0.0:9090"))]
    address: SocketAddr,

    /// The URL path on which metric POST requests are accepted.
    #[serde(default = "default_path")]
    #[configurable(metadata(docs::examples = "/api/v1/write"))]
    #[configurable(metadata(docs::examples = "/remote-write"))]
    path: String,

    #[configurable(derived)]
    tls: Option<TlsEnableableConfig>,

    #[configurable(derived)]
    auth: Option<HttpServerAuthConfig>,

    /// Defines the behavior for handling conflicting metric metadata.
    #[serde(default)]
    metadata_conflict_strategy: MetadataConflictStrategy,

    #[configurable(derived)]
    #[serde(default, deserialize_with = "bool_or_struct")]
    acknowledgements: SourceAcknowledgementsConfig,

    #[configurable(derived)]
    #[serde(default)]
    keepalive: KeepaliveConfig,

    /// Whether to skip/discard received samples with NaN values.
    ///
    /// When enabled, any metric sample with a NaN value will be filtered out
    /// during parsing, preventing downstream processing of invalid metrics.
    #[serde(default)]
    skip_nan_values: bool,
}

impl PrometheusRemoteWriteConfig {
    #[cfg(test)]
    pub fn from_address(address: SocketAddr) -> Self {
        Self {
            address,
            path: default_path(),
            tls: None,
            auth: None,
            metadata_conflict_strategy: MetadataConflictStrategy::default(),
            acknowledgements: false.into(),
            keepalive: KeepaliveConfig::default(),
            skip_nan_values: false,
        }
    }
}

fn default_path() -> String {
    "/".to_string()
}

impl GenerateConfig for PrometheusRemoteWriteConfig {
    fn generate_config() -> serde_json::Value {
        serde_json::to_value(Self {
            address: "127.0.0.1:9090".parse().unwrap(),
            path: default_path(),
            tls: None,
            auth: None,
            metadata_conflict_strategy: MetadataConflictStrategy::default(),
            acknowledgements: SourceAcknowledgementsConfig::default(),
            keepalive: KeepaliveConfig::default(),
            skip_nan_values: false,
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "prometheus_remote_write")]
impl SourceConfig for PrometheusRemoteWriteConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<sources::Source> {
        let source = RemoteWriteSource {
            metadata_conflict_strategy: self.metadata_conflict_strategy,
            skip_nan_values: self.skip_nan_values,
            log_namespace: cx.log_namespace(None),
        };
        source.run(
            self.address,
            self.path.as_str(),
            HttpMethod::Post,
            StatusCode::OK,
            true,
            self.tls.as_ref(),
            self.auth.as_ref(),
            cx,
            self.acknowledgements,
            self.keepalive.clone(),
        )
    }

    fn outputs(&self, _global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        vec![SourceOutput::new_metrics()]
    }

    fn can_acknowledge(&self) -> bool {
        true
    }
}

#[derive(Clone)]
struct RemoteWriteSource {
    metadata_conflict_strategy: MetadataConflictStrategy,
    skip_nan_values: bool,
    log_namespace: LogNamespace,
}

impl RemoteWriteSource {
    fn decode_body(&self, body: Bytes) -> Result<Vec<Event>, ErrorMessage> {
        let request = proto::WriteRequest::decode(body).map_err(|error| {
            emit!(PrometheusRemoteWriteParseError {
                error: error.clone()
            });
            ErrorMessage::new(
                StatusCode::BAD_REQUEST,
                format!("Could not decode write request: {error}"),
            )
        })?;
        parser::parse_request(
            request,
            self.metadata_conflict_strategy,
            self.skip_nan_values,
        )
        .map_err(|error| {
            ErrorMessage::new(
                StatusCode::BAD_REQUEST,
                format!("Could not decode write request: {error}"),
            )
        })
    }
}

impl HttpSource for RemoteWriteSource {
    fn log_namespace(&self) -> LogNamespace {
        self.log_namespace
    }

    fn name() -> &'static str {
        PrometheusRemoteWriteConfig::NAME
    }

    fn decode(&self, encoding_header: Option<&str>, body: Bytes) -> Result<Bytes, ErrorMessage> {
        // Default to snappy decoding the request body.
        decompress_body(encoding_header.or(Some("snappy")), body)
    }

    fn build_events(
        &self,
        body: Bytes,
        _header_map: &HeaderMap,
        _query_parameters: &HashMap<String, String>,
        _full_path: &str,
    ) -> Result<Vec<Event>, ErrorMessage> {
        let events = self.decode_body(body)?;
        Ok(events)
    }
}

#[cfg(test)]
mod test;

#[cfg(all(test, feature = "prometheus-integration-tests"))]
mod integration_tests;
