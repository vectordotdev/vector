use std::{
    num::NonZeroU64,
};

use http::Uri;
use vector_lib::{configurable::configurable_component};

use super::{
    push::compression::VectorCompression,
};
use crate::{
    config::{
        AcknowledgementsConfig, GenerateConfig, Input, SinkConfig, SinkContext,
    },
    sinks::{
        Healthcheck, VectorSink as VectorSinkType,
        util::{
            BatchConfig, RealtimeEventBasedDefaultBatchSettings, TowerRequestConfig,
            service::{HealthConfig},
        },
    },
    tls::{TlsEnableableConfig},
};

/// Marker type for the version two of the configuration for the `vector` sink.
#[configurable_component]
#[derive(Clone, Debug)]
pub enum VectorConfigVersion {
    /// Marker value for version two.
    #[serde(rename = "2")]
    V2,
}

#[configurable_component()]
#[configurable(description = "vector mode")]
#[derive(Clone, Debug)]
pub enum VectorMode {
    #[serde(rename = "push")]
    #[configurable(description = "something")]
    Push,
    #[serde(rename = "serve")]
    #[configurable(description = "something")]
    Serve,
}

impl Default for VectorMode {
    fn default() -> Self {
        Self::Push
    }
}

/// Configuration for the `vector` sink.
#[configurable_component(sink("vector", "Relay observability data to a Vector instance."))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct VectorConfig {
    /// Version of the configuration.
    // NOTE: this option is deprecated and has already been removed from the "old" docs.
    // At some point in the future we will remove it entirely as a breaking change.
    #[configurable(metadata(docs::hidden))]
    version: Option<super::VectorConfigVersion>,

    /// Mode
    #[serde(default)]
    mode: VectorMode,

    /// The downstream Vector address to which to connect.
    ///
    /// Both IP address and hostname are accepted formats.
    ///
    /// The address _must_ include a port.
    ///
    /// This option is mutually exclusive with `routing`. Set exactly one of
    /// `address` or `routing`.
    ///
    /// This option has been deprecated, use `routing.endpoints` instead.
    #[configurable(validation(format = "uri"))]
    #[configurable(
        deprecated = "This option has been deprecated, use `routing.endpoints` instead."
    )]
    #[configurable(metadata(docs::examples = "92.12.333.224:6000"))]
    #[configurable(metadata(docs::examples = "https://somehost:6000"))]
    #[serde(default)]
    pub address: Option<String>,

    /// Routing options for sending requests to one or more downstream Vector endpoints.
    ///
    /// This option is mutually exclusive with `address`. Set exactly one of
    /// `address` or `routing`.
    #[serde(default)]
    #[configurable(derived)]
    pub routing: Option<RoutingConfig>,

    /// Compression algorithm for requests.
    ///
    /// Supports `"none"`, `"gzip"`, or `"zstd"`.
    ///
    /// For backward compatibility, boolean values are still accepted:
    /// - `true` defaults to gzip compression
    /// - `false` disables compression (deprecated syntax)
    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "super::push::compression::bool_or_vector_compression"
    )]
    pub compression: VectorCompression,

    #[configurable(derived)]
    #[serde(default)]
    pub batch: BatchConfig<RealtimeEventBasedDefaultBatchSettings>,

    #[configurable(derived)]
    #[serde(default)]
    pub request: TowerRequestConfig,

    #[configurable(derived)]
    #[serde(default)]
    pub tls: Option<TlsEnableableConfig>,

    /// HTTP/2 keepalive settings for the sink's gRPC connections.
    ///
    /// Keepalive is disabled unless this is configured. When enabled, the sink sends HTTP/2 PING
    /// frames on idle connections so that a pooled connection to a downstream Vector instance that
    /// has gone away (crashed, restarted, or cut off by a network partition) is detected and evicted
    /// before it is reused, ensuring retries always go to a live connection.
    #[configurable(derived)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keepalive: Option<VectorKeepaliveConfig>,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub(in crate::sinks::vector) acknowledgements: AcknowledgementsConfig,
}

/// HTTP/2 keepalive configuration for the `vector` sink's gRPC connections.
#[configurable_component]
#[derive(Clone, Copy, Debug)]
#[serde(deny_unknown_fields)]
pub struct VectorKeepaliveConfig {
    /// How often, in seconds, to send a keepalive PING on idle connections.
    ///
    /// Shorter intervals detect dead connections faster at the cost of additional traffic.
    /// gRPC guidance recommends no less than 60 seconds to avoid tripping `too_many_pings`
    /// policies on servers or proxies between source and destination.
    #[serde(default = "default_keepalive_interval_secs")]
    #[configurable(metadata(docs::human_name = "Keepalive Interval"))]
    pub interval_secs: NonZeroU64,

    /// How long, in seconds, to wait for a keepalive PING acknowledgement before treating
    /// the connection as dead and closing it.
    #[serde(default = "default_keepalive_timeout_secs")]
    #[configurable(metadata(docs::human_name = "Keepalive Timeout"))]
    pub timeout_secs: NonZeroU64,
}

const fn default_keepalive_interval_secs() -> NonZeroU64 {
    // Aligned with gRPC keepalive guidance, which recommends no less than one minute to avoid
    // tripping `too_many_pings` policies on proxies between the sink and downstream.
    NonZeroU64::new(60).expect("keepalive interval default must be nonzero")
}

const fn default_keepalive_timeout_secs() -> NonZeroU64 {
    // Matches hyper's default keepalive timeout.
    NonZeroU64::new(20).expect("keepalive timeout default must be nonzero")
}

/// Routing options for sending requests to downstream Vector endpoints.
///
/// Load-balanced sinks healthcheck all configured endpoints on startup.
/// Failover sinks healthcheck only the initially active endpoint by default,
/// which is the first configured endpoint, unless `healthcheck.uri` is set.
#[configurable_component]
#[derive(Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct RoutingConfig {
    /// The downstream Vector endpoints to which to connect.
    ///
    /// Both IP addresses and hostnames are accepted formats.
    ///
    /// Each endpoint _must_ include a port.
    #[configurable(validation(format = "uri"))]
    #[configurable(metadata(docs::examples = "92.12.333.224:6000"))]
    #[configurable(metadata(docs::examples = "https://somehost:6000"))]
    #[serde(default)]
    endpoints: Vec<String>,

    /// Strategy for routing requests across configured endpoints.
    ///
    /// When only one endpoint is configured, the sink uses the standard
    /// single-endpoint service path and strategy-specific routing semantics are
    /// not applied.
    #[serde(default)]
    pub strategy: super::push::EndpointStrategy,

    /// Options for determining the health and backoff behavior of
    /// load-balanced Vector endpoints.
    ///
    /// This option is only used when `strategy` is set to `load_balance`.
    #[serde(default)]
    #[configurable(derived)]
    pub health: Option<HealthConfig>,
}

impl VectorConfig {
    /// Creates a `VectorConfig` with the given address.
    pub fn from_address(addr: Uri) -> Self {
        let addr = addr.to_string();
        default_config(addr.as_str())
    }
}

impl GenerateConfig for VectorConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(default_config("127.0.0.1:6000")).unwrap()
    }
}

fn default_config(address: &str) -> VectorConfig {
    VectorConfig {
        version: None,
        mode: VectorMode::Push,
        address: Some(address.to_owned()),
        routing: None,
        compression: VectorCompression::None,
        batch: BatchConfig::default(),
        request: TowerRequestConfig::default(),
        tls: None,
        keepalive: None,
        acknowledgements: Default::default(),
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "vector")]
impl SinkConfig for VectorConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSinkType, Healthcheck)> {
        match self.mode {
            VectorMode::Push => {
                super::push::config_to_push_sink(self, cx)
            }
            VectorMode::Serve => {
                super::serve::config_to_serve_sink(self).await
            }
        }
    }

    fn input(&self) -> Input {
        Input::all()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}


/// grpc doesn't like an address without a scheme, so we default to http or https if one isn't
/// specified in the address.
pub fn with_default_scheme(address: &str, tls: bool) -> crate::Result<http::Uri> {
    let uri = address.parse::<http::Uri>()?;
    if uri.scheme().is_none() {
        // Default the scheme to http or https.
        let mut parts = uri.into_parts();

        parts.scheme = if tls {
            Some(
                "https"
                    .parse()
                    .unwrap_or_else(|_| unreachable!("https should be valid")),
            )
        } else {
            Some(
                "http"
                    .parse()
                    .unwrap_or_else(|_| unreachable!("http should be valid")),
            )
        };

        if parts.path_and_query.is_none() {
            parts.path_and_query = Some(
                "/".parse()
                    .unwrap_or_else(|_| unreachable!("root should be valid")),
            );
        }
        Ok(http::Uri::from_parts(parts)?)
    } else {
        Ok(uri)
    }
}

impl VectorConfig {
    fn validate_endpoint_options(&self) -> crate::Result<()> {
        match (self.address.as_ref(), self.routing.as_ref()) {
            (Some(_), Some(_)) => Err(
                "`address` and `routing` options are mutually exclusive. Please use `routing.endpoints` for multiple Vector endpoints."
                    .into(),
            ),
            (None, None) => {
                Err("No Vector endpoint configured. Please set `address` or `routing.endpoints`.".into())
            }
            (None, Some(routing)) if routing.endpoints.is_empty() => {
                Err("`routing.endpoints` must contain at least one endpoint.".into())
            }
            (Some(_), None) | (None, Some(_)) => Ok(()),
        }
    }

    pub fn uris(&self, tls: bool) -> crate::Result<Vec<Uri>> {
        self.validate_endpoint_options()?;

        if let Some(address) = self.address.as_ref() {
            Ok(vec![with_default_scheme(address, tls)?])
        } else {
            self.routing
                .as_ref()
                .expect("routing must be present after validation")
                .endpoints
                .iter()
                .map(|endpoint| with_default_scheme(endpoint, tls))
                .collect()
        }
    }
}

