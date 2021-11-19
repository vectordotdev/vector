use crate::sinks::util::{BatchConfig, RealtimeEventBasedDefaultBatchSettings, TowerRequestConfig};
use crate::tls::{TlsConfig, MaybeTlsSettings, tls_connector_builder};
use crate::config::{GenerateConfig, SinkContext, ProxyConfig, DataType, Resource, SinkHealthcheckOptions};
use http::Uri;
use crate::sinks::{Healthcheck, VectorSink as VectorSinkType};
use hyper_openssl::HttpsConnector;
use hyper_proxy::ProxyConnector;
use hyper::client::HttpConnector;
use tonic::body::BoxBody;
use crate::sinks::util::retries::RetryLogic;
use crate::proto::vector as proto;
use crate::sinks::vector::v2::service::VectorService;
use crate::sinks::vector::v2::sink::VectorSink;
use serde::{Serialize, Deserialize};
use snafu::Snafu;
use crate::sinks::vector::v2::VectorSinkError;
use tower::ServiceBuilder;
use crate::Error;


#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct VectorConfig {
    address: String,
    #[serde(default)]
    pub batch: BatchConfig<RealtimeEventBasedDefaultBatchSettings>,
    #[serde(default)]
    pub request: TowerRequestConfig,
    #[serde(default)]
    tls: Option<TlsConfig>,
}



impl GenerateConfig for VectorConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(default_config("127.0.0.1:6000")).unwrap()
    }
}

fn default_config(address: &str) -> VectorConfig {
    VectorConfig {
        address: address.to_owned(),
        batch: BatchConfig::default(),
        request: TowerRequestConfig::default(),
        tls: None,
    }
}

impl VectorConfig {
    pub(crate) async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSinkType, Healthcheck)> {
        let tls = MaybeTlsSettings::from_config(&self.tls, false)?;
        let uri = with_default_scheme(&self.address, tls.is_tls())?;

        let client = new_client(&tls, cx.proxy())?;

        let healthcheck_uri = cx
            .healthcheck
            .uri
            .clone()
            .map(|uri| uri.uri)
            .unwrap_or_else(|| uri.clone());
        let healthcheck_client = VectorService::new(client.clone(), healthcheck_uri);
        let healthcheck = healthcheck(healthcheck_client, cx.healthcheck.clone());
        let service = VectorService::new(client, uri);
        let request_settings = self.request.unwrap_with(&TowerRequestConfig::default());
        let batch_settings = self.batch.into_batcher_settings()?;
        //
        let service = ServiceBuilder::new()
            .settings(request_settings, VectorGrpcRetryLogic)
            .service(service);

        let sink = VectorSink {
            batch_settings,
            service,
            acker: cx.acker()
        };

        Ok((VectorSinkType::Stream(Box::new(sink)), Box::pin(healthcheck)))
    }

    pub(super) const fn input_type(&self) -> DataType {
        DataType::Any
    }

    pub(super) const fn sink_type(&self) -> &'static str {
        "vector"
    }

    pub(super) const fn resources(&self) -> Vec<Resource> {
        Vec::new()
    }
}


/// Check to see if the remote service accepts new events.
//TODO: use proto::Client instead of service?
async fn healthcheck(mut service: VectorService, options: SinkHealthcheckOptions) -> crate::Result<()> {
    if !options.enabled {
        return Ok(());
    }

    let request = service.client.health_check(proto::HealthCheckRequest {});

    if let Ok(response) = request.await {
        let status = proto::ServingStatus::from_i32(response.into_inner().status);

        if let Some(proto::ServingStatus::Serving) = status {
            return Ok(());
        }
    }

    Err(Box::new(VectorSinkError::Health))
}

/// grpc doesn't like an address without a scheme, so we default to http or https if one isn't
/// specified in the address.
fn with_default_scheme(address: &str, tls: bool) -> crate::Result<Uri> {
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
    let mut http = HttpConnector::new();
    http.enforce_http(false);

    let tls = tls_connector_builder(tls_settings)?;
    let mut https = HttpsConnector::with_connector(http, tls)?;

    let settings = tls_settings.tls().cloned();
    https.set_callback(move |c, _uri| {
        if let Some(settings) = &settings {
            settings.apply_connect_configuration(c);
        }

        Ok(())
    });

    let mut proxy = ProxyConnector::new(https).unwrap();
    proxy_config.configure(&mut proxy)?;

    Ok(hyper::Client::builder().http2_only(true).build(proxy))
}

#[derive(Debug, Clone)]
struct VectorGrpcRetryLogic;

impl RetryLogic for VectorGrpcRetryLogic {
    type Error = Error;
    type Response = ();

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
            ),
            _ => true,
        }
    }
}
