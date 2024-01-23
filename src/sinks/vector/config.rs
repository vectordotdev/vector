use http::Uri;
use hyper::client::HttpConnector;
use hyper_openssl::HttpsConnector;
use hyper_proxy::ProxyConnector;
use tonic::body::BoxBody;
use tower::ServiceBuilder;
use vector_lib::configurable::configurable_component;

use super::{
    service::{VectorResponse, VectorService},
    sink::VectorSink,
    VectorSinkError,
};
use crate::{
    config::{
        AcknowledgementsConfig, GenerateConfig, Input, ProxyConfig, SinkConfig, SinkContext,
        SinkHealthcheckOptions,
    },
    http::build_proxy_connector,
    proto::vector as proto,
    sinks::{
        util::{
            retries::RetryLogic, BatchConfig, RealtimeEventBasedDefaultBatchSettings,
            ServiceBuilderExt, TowerRequestConfig,
        },
        Healthcheck, VectorSink as VectorSinkType,
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

    /// Whether or not to compress requests.
    ///
    /// If set to `true`, requests are compressed with [`gzip`][gzip_docs].
    ///
    /// [gzip_docs]: https://www.gzip.org/
    #[configurable(metadata(docs::advanced))]
    #[serde(default)]
    compression: bool,

    #[configurable(derived)]
    #[serde(default)]
    pub batch: BatchConfig<RealtimeEventBasedDefaultBatchSettings>,

    #[configurable(derived)]
    #[serde(default)]
    pub request: TowerRequestConfig,

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
        compression: false,
        batch: BatchConfig::default(),
        request: TowerRequestConfig::default(),
        tls: None,
        acknowledgements: Default::default(),
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "vector")]
impl SinkConfig for VectorConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSinkType, Healthcheck)> {
        let tls = MaybeTlsSettings::from_config(&self.tls, false)?;
        let uri = with_default_scheme(&self.address, tls.is_tls())?;

        let client = new_client(&tls, cx.proxy())?;

        let healthcheck_uri = cx
            .healthcheck
            .uri
            .clone()
            .map(|uri| uri.uri)
            .unwrap_or_else(|| uri.clone());
        let healthcheck_client = VectorService::new(client.clone(), healthcheck_uri, false);
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

impl RetryLogic for VectorGrpcRetryLogic {
    type Error = VectorSinkError;
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
