use std::num::NonZeroU64;
use std::time::Duration;

use http::Uri;
use hyper::client::HttpConnector;
use hyper_openssl::HttpsConnector;
use hyper_proxy::ProxyConnector;
use tonic::body::BoxBody;
use tower::ServiceBuilder;
use vector_lib::configurable::configurable_component;

use super::{
    VectorSinkError,
    compression::VectorCompression,
    service::{VectorRequest, VectorResponse, VectorService},
    sink::VectorSink,
};
use crate::{
    config::{
        AcknowledgementsConfig, GenerateConfig, Input, ProxyConfig, SinkConfig, SinkContext,
        SinkHealthcheckOptions,
    },
    http::build_proxy_connector,
    proto::vector as proto,
    sinks::{
        Healthcheck, VectorSink as VectorSinkType,
        util::{
            BatchConfig, RealtimeEventBasedDefaultBatchSettings, ServiceBuilderExt,
            TowerRequestConfig, retries::RetryLogic,
        },
    },
    tls::{MaybeTlsSettings, TlsEnableableConfig},
};

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

    /// The downstream Vector address to which to connect.
    ///
    /// Both IP address and hostname are accepted formats.
    ///
    /// The address _must_ include a port.
    #[configurable(validation(format = "uri"))]
    #[configurable(metadata(docs::examples = "92.12.333.224:6000"))]
    #[configurable(metadata(docs::examples = "https://somehost:6000"))]
    address: String,

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
        deserialize_with = "super::compression::bool_or_vector_compression"
    )]
    compression: VectorCompression,

    #[configurable(derived)]
    #[serde(default)]
    pub batch: BatchConfig<RealtimeEventBasedDefaultBatchSettings>,

    #[configurable(derived)]
    #[serde(default)]
    pub request: TowerRequestConfig,

    #[configurable(derived)]
    #[serde(default)]
    tls: Option<TlsEnableableConfig>,

    /// HTTP/2 keepalive settings for the sink's gRPC connections.
    ///
    /// Keepalive is disabled unless this is configured. When enabled, the sink sends HTTP/2 PING
    /// frames on idle connections so that a pooled connection to a downstream Vector instance that
    /// has gone away (crashed, restarted, or cut off by a network partition) is detected and evicted
    /// before it is reused, ensuring retries always go to a live connection.
    #[configurable(derived)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    keepalive: Option<VectorKeepaliveConfig>,

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
        address: address.to_owned(),
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
        let tls = MaybeTlsSettings::from_config(self.tls.as_ref(), false)?;
        let uri = with_default_scheme(&self.address, tls.is_tls())?;

        let client = new_client(&tls, cx.proxy(), self.keepalive)?;

        let healthcheck_uri = cx
            .healthcheck
            .uri
            .clone()
            .map(|uri| uri.uri)
            .unwrap_or_else(|| uri.clone());
        let healthcheck_client =
            VectorService::new(client.clone(), healthcheck_uri, VectorCompression::None);
        let healthcheck = healthcheck(healthcheck_client, cx.healthcheck);
        let service = VectorService::new(client, uri, self.compression);
        let request_settings = self.request.into_settings();
        let batch_settings = self.batch.into_batcher_settings()?;

        let service = ServiceBuilder::new()
            .settings(request_settings, VectorGrpcRetryLogic)
            .service(service);

        let sink = VectorSink {
            batch_settings,
            service,
        };

        Ok((
            VectorSinkType::from_event_streamsink(sink),
            Box::pin(healthcheck),
        ))
    }

    fn input(&self) -> Input {
        Input::all()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

/// Check to see if the remote service accepts new events.
async fn healthcheck(
    mut service: VectorService,
    options: SinkHealthcheckOptions,
) -> crate::Result<()> {
    if !options.enabled {
        return Ok(());
    }

    // Use the custom Vector health check
    // Note: Both custom and standard health checks behave identically - they just
    // return serving status without actual health validation. The Vector source
    // implements both protocols now for compatibility.
    let request = service.client.health_check(proto::HealthCheckRequest {});
    match request.await {
        Ok(response) => match proto::ServingStatus::try_from(response.into_inner().status) {
            Ok(proto::ServingStatus::Serving) => Ok(()),
            Ok(status) => Err(Box::new(VectorSinkError::Health {
                status: Some(status.as_str_name()),
            })),
            Err(_) => Err(Box::new(VectorSinkError::Health { status: None })),
        },
        Err(source) => Err(Box::new(VectorSinkError::Request { source })),
    }
}

/// grpc doesn't like an address without a scheme, so we default to http or https if one isn't
/// specified in the address.
pub fn with_default_scheme(address: &str, tls: bool) -> crate::Result<Uri> {
    let uri: Uri = address.parse()?;
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
        Ok(Uri::from_parts(parts)?)
    } else {
        Ok(uri)
    }
}

fn new_client(
    tls_settings: &MaybeTlsSettings,
    proxy_config: &ProxyConfig,
    keepalive: Option<VectorKeepaliveConfig>,
) -> crate::Result<hyper::Client<ProxyConnector<HttpsConnector<HttpConnector>>, BoxBody>> {
    let proxy = build_proxy_connector(tls_settings.clone(), proxy_config)?;

    let mut builder = hyper::Client::builder();
    builder.http2_only(true);

    // Keepalive is opt-in. When enabled, PINGs are sent on idle connections so dead connections
    // are detected and evicted before they are reused, not during a request.
    if let Some(keepalive) = keepalive {
        builder
            .http2_keep_alive_interval(Duration::from_secs(keepalive.interval_secs.get()))
            .http2_keep_alive_timeout(Duration::from_secs(keepalive.timeout_secs.get()))
            // Always ping idle connections: the downstream is always a Vector instance, which
            // won't reject pings without active calls, so idle-keepalive is always safe here.
            .http2_keep_alive_while_idle(true);
    }

    Ok(builder.build(proxy))
}

#[derive(Debug, Clone)]
struct VectorGrpcRetryLogic;

impl RetryLogic for VectorGrpcRetryLogic {
    type Error = VectorSinkError;
    type Request = VectorRequest;
    type Response = VectorResponse;

    fn is_retriable_error(&self, err: &Self::Error) -> bool {
        use tonic::Code::*;

        match err {
            VectorSinkError::Request { source } => !matches!(
                source.code(),
                // List taken from
                //
                // <https://github.com/grpc/grpc/blob/ed1b20777c69bd47e730a63271eafc1b299f6ca0/doc/statuscodes.md>
                NotFound
                    | InvalidArgument
                    | AlreadyExists
                    | PermissionDenied
                    | OutOfRange
                    | Unimplemented
                    | Unauthenticated
                    | DataLoss
            ),
            _ => true,
        }
    }
}
