use futures_util::FutureExt;
use http::StatusCode;
use tower::Service;
use vector_config::configurable_component;
use vector_core::config::{AcknowledgementsConfig, Input};
use vector_core::tls::{TlsConfig, TlsSettings};

use crate::config::{SinkConfig, SinkContext};
use crate::http::HttpClient;
use crate::sinks::util::{BatchConfig, SinkBatchSettings};
use crate::sinks::{Healthcheck, VectorSink};

use super::util::TowerRequestConfig;

mod client;

#[derive(Clone, Copy, Debug, Default)]
pub struct GreptimeDBDefaultBatchSettings;

impl SinkBatchSettings for GreptimeDBDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = Some(20);
    const MAX_BYTES: Option<usize> = None;
    const TIMEOUT_SECS: f64 = 1.0;
}

/// Configuration items for GreptimeDB
#[configurable_component(sink("greptimedb_metrics"))]
#[derive(Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct GreptimeDBConfig {
    /// The catalog name to connect
    #[configurable(metadata(docs::examples = "greptime"))]
    pub catalog: String,
    /// The schema name to connect
    #[configurable(metadata(docs::examples = "public"))]
    pub schema: String,
    /// The host and port of greptimedb grpc service
    #[configurable(metadata(docs::examples = "localhost:4001"))]
    pub grpc_endpoint: String,
    /// The host and port of greptimedb http service
    #[configurable(metadata(docs::examples = "http://localhost:4000"))]
    pub http_endpoint: String,

    #[configurable(derived)]
    #[serde(default)]
    pub request: TowerRequestConfig,

    #[configurable(derived)]
    #[serde(default)]
    pub batch: BatchConfig<GreptimeDBDefaultBatchSettings>,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,

    #[configurable(derived)]
    pub tls: Option<TlsConfig>,
}

impl_generate_config_from_default!(GreptimeDBConfig);

#[async_trait::async_trait]
impl SinkConfig for GreptimeDBConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let sink = client::GreptimeDBService::new_sink(&self)?;
        let tls_settings = TlsSettings::from_options(&self.tls)?;
        let http_client = HttpClient::new(tls_settings, cx.proxy())?;
        let healthcheck = healthcheck(&self.http_endpoint, http_client)?;
        Ok((sink, healthcheck))
    }

    fn input(&self) -> Input {
        Input::metric()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

fn healthcheck(endpoint: &str, mut client: HttpClient) -> crate::Result<super::Healthcheck> {
    let uri = format!("{endpoint}/health");

    let request = hyper::Request::get(uri).body(hyper::Body::empty()).unwrap();

    Ok(async move {
        client
            .call(request)
            .await
            .map_err(|error| error.into())
            .and_then(|response| match response.status() {
                StatusCode::OK => Ok(()),
                StatusCode::NO_CONTENT => Ok(()),
                other => Err(super::HealthcheckError::UnexpectedStatus { status: other }.into()),
            })
    }
    .boxed())
}
