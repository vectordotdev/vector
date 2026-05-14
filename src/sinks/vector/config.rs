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
        util::service::{HealthConfig, HealthLogic},
        util::{
            BatchConfig, RealtimeEventBasedDefaultBatchSettings, ServiceBuilderExt,
            TowerRequestConfig, retries::RetryLogic,
        },
    },
    tls::{MaybeTlsSettings, TlsEnableableConfig},
};

const fn default_connection_concurrency() -> usize {
    1
}

/// Connection settings for the `vector` sink.
#[configurable_component]
#[derive(Clone, Copy, Debug)]
#[serde(deny_unknown_fields)]
pub struct VectorConnectionConfig {
    /// The number of outbound gRPC connections to open to the configured endpoint.
    #[serde(default = "default_connection_concurrency")]
    #[configurable(validation(range(min = 1)))]
    pub concurrency: usize,
}

impl Default for VectorConnectionConfig {
    fn default() -> Self {
        Self {
            concurrency: default_connection_concurrency(),
        }
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
    pub connection: VectorConnectionConfig,

    #[configurable(derived)]
    #[serde(default)]
    tls: Option<TlsEnableableConfig>,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub(in crate::sinks::vector) acknowledgements: AcknowledgementsConfig,
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
        connection: VectorConnectionConfig::default(),
        tls: None,
        acknowledgements: Default::default(),
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "vector")]
impl SinkConfig for VectorConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSinkType, Healthcheck)> {
        if self.connection.concurrency == 0 {
            return Err(Box::new(VectorSinkError::InvalidConnectionConcurrency {
                value: self.connection.concurrency,
            }));
        }

        let proxy = cx.proxy().clone();
        let healthcheck_options = cx.healthcheck.clone();
        let tls = MaybeTlsSettings::from_config(self.tls.as_ref(), false)?;
        let uri = with_default_scheme(&self.address, tls.is_tls())?;

        let healthcheck_uri = healthcheck_options
            .uri
            .clone()
            .map(|uri| uri.uri)
            .unwrap_or_else(|| uri.clone());
        let healthcheck_client = VectorService::new(
            new_client(&tls, &proxy)?,
            healthcheck_uri,
            VectorCompression::None,
        );
        let healthcheck = healthcheck(healthcheck_client, healthcheck_options);
        let request_settings = self.request.into_settings();
        let sink = if self.connection.concurrency == 1 {
            let client = new_client(&tls, &proxy)?;
            let service = VectorService::new(client, uri, self.compression);
            let service = ServiceBuilder::new()
                .settings(request_settings, VectorGrpcRetryLogic)
                .service(service);

            VectorSinkType::from_event_streamsink(VectorSink {
                batch_settings: self.batch.into_batcher_settings()?,
                service,
            })
        } else {
            let endpoint = uri.to_string();
            let services = (0..self.connection.concurrency)
                .map(|_| {
                    let client = new_client(&tls, &proxy)?;
                    Ok((
                        endpoint.clone(),
                        VectorService::new(client, uri.clone(), self.compression),
                    ))
                })
                .collect::<crate::Result<Vec<_>>>()?;

            let service = request_settings.distributed_service(
                VectorGrpcRetryLogic,
                services,
                HealthConfig::default(),
                VectorHealthLogic,
                1,
            );

            VectorSinkType::from_event_streamsink(VectorSink {
                batch_settings: self.batch.into_batcher_settings()?,
                service,
            })
        };

        Ok((sink, Box::pin(healthcheck)))
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
) -> crate::Result<hyper::Client<ProxyConnector<HttpsConnector<HttpConnector>>, BoxBody>> {
    let proxy = build_proxy_connector(tls_settings.clone(), proxy_config)?;

    Ok(hyper::Client::builder().http2_only(true).build(proxy))
}

#[derive(Debug, Clone)]
struct VectorGrpcRetryLogic;

fn is_permanent_grpc_status(code: tonic::Code) -> bool {
    use tonic::Code::*;

    matches!(
        code,
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
    )
}

impl RetryLogic for VectorGrpcRetryLogic {
    type Error = VectorSinkError;
    type Request = VectorRequest;
    type Response = VectorResponse;

    fn is_retriable_error(&self, err: &Self::Error) -> bool {
        match err {
            VectorSinkError::Request { source } => !is_permanent_grpc_status(source.code()),
            _ => true,
        }
    }
}

#[derive(Clone)]
struct VectorHealthLogic;

impl HealthLogic for VectorHealthLogic {
    type Error = crate::Error;
    type Response = VectorResponse;

    fn is_healthy(&self, response: &Result<Self::Response, Self::Error>) -> Option<bool> {
        match response {
            Ok(_) => Some(true),
            Err(error) => error
                .downcast_ref::<VectorSinkError>()
                .and_then(|err| match err {
                    VectorSinkError::Request { source } => {
                        if is_permanent_grpc_status(source.code()) {
                            None
                        } else {
                            Some(false)
                        }
                    }
                    _ => None,
                }),
        }
    }
}

#[cfg(test)]
mod tests {
    use tonic::{Code, Status};

    use super::*;

    #[test]
    fn grpc_retry_logic_does_not_retry_permanent_statuses() {
        let logic = VectorGrpcRetryLogic;

        assert!(!logic.is_retriable_error(&VectorSinkError::Request {
            source: Status::new(Code::InvalidArgument, "invalid request"),
        }));
        assert!(logic.is_retriable_error(&VectorSinkError::Request {
            source: Status::new(Code::Unavailable, "temporarily unavailable"),
        }));
    }

    #[test]
    fn grpc_health_logic_only_marks_transient_statuses_unhealthy() {
        let logic = VectorHealthLogic;

        assert_eq!(
            logic.is_healthy(&Err(Box::new(VectorSinkError::Request {
                source: Status::new(Code::InvalidArgument, "invalid request"),
            }))),
            None
        );
        assert_eq!(
            logic.is_healthy(&Err(Box::new(VectorSinkError::Request {
                source: Status::new(Code::Unavailable, "temporarily unavailable"),
            }))),
            Some(false)
        );
    }
}
