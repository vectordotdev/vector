use http::Uri;
use snafu::prelude::*;

use crate::{
    http::HttpClient,
    sinks::{
        prelude::*,
        prometheus::PrometheusRemoteWriteAuth,
        util::{auth::Auth, http::http_response_retry_logic},
        UriParseSnafu,
    },
};

use super::{
    service::{build_request, RemoteWriteService},
    sink::{PrometheusRemoteWriteDefaultBatchSettings, RemoteWriteSink},
};

#[cfg(feature = "aws-core")]
use super::Errors;

/// The batch config for remote write.
#[configurable_component]
#[derive(Clone, Copy, Debug, Derivative)]
#[derivative(Default)]
pub struct RemoteWriteBatchConfig {
    #[configurable(derived)]
    #[serde(flatten)]
    pub batch_settings: BatchConfig<PrometheusRemoteWriteDefaultBatchSettings>,

    /// Whether or not to aggregate metrics within a batch.
    #[serde(default = "crate::serde::default_true")]
    #[derivative(Default(value = "true"))]
    pub aggregate: bool,
}

/// Configuration for the `prometheus_remote_write` sink.
#[configurable_component(sink(
    "prometheus_remote_write",
    "Deliver metric data to a Prometheus remote write endpoint."
))]
#[derive(Clone, Debug, Derivative)]
#[derivative(Default)]
#[serde(deny_unknown_fields)]
pub struct RemoteWriteConfig {
    /// The endpoint to send data to.
    ///
    /// The endpoint should include the scheme and the path to write to.
    #[configurable(metadata(docs::examples = "https://localhost:8087/api/v1/write"))]
    pub endpoint: String,

    /// The default namespace for any metrics sent.
    ///
    /// This namespace is only used if a metric has no existing namespace. When a namespace is
    /// present, it is used as a prefix to the metric name, and separated with an underscore (`_`).
    ///
    /// It should follow the Prometheus [naming conventions][prom_naming_docs].
    ///
    /// [prom_naming_docs]: https://prometheus.io/docs/practices/naming/#metric-names
    #[configurable(metadata(docs::examples = "service"))]
    #[configurable(metadata(docs::advanced))]
    pub default_namespace: Option<String>,

    /// Default buckets to use for aggregating [distribution][dist_metric_docs] metrics into histograms.
    ///
    /// [dist_metric_docs]: https://vector.dev/docs/about/under-the-hood/architecture/data-model/metric/#distribution
    #[serde(default = "crate::sinks::prometheus::default_histogram_buckets")]
    #[configurable(metadata(docs::advanced))]
    pub buckets: Vec<f64>,

    /// Quantiles to use for aggregating [distribution][dist_metric_docs] metrics into a summary.
    ///
    /// [dist_metric_docs]: https://vector.dev/docs/about/under-the-hood/architecture/data-model/metric/#distribution
    #[serde(default = "crate::sinks::prometheus::default_summary_quantiles")]
    #[configurable(metadata(docs::advanced))]
    pub quantiles: Vec<f64>,

    #[configurable(derived)]
    #[serde(default)]
    pub batch: RemoteWriteBatchConfig,

    #[configurable(derived)]
    #[serde(default)]
    pub request: TowerRequestConfig,

    /// The tenant ID to send.
    ///
    /// If set, a header named `X-Scope-OrgID` is added to outgoing requests with the value of this setting.
    ///
    /// This may be used by Cortex or other remote services to identify the tenant making the request.
    #[serde(default)]
    #[configurable(metadata(docs::examples = "my-domain"))]
    #[configurable(metadata(docs::advanced))]
    pub tenant_id: Option<Template>,

    #[configurable(derived)]
    pub tls: Option<TlsConfig>,

    #[configurable(derived)]
    pub auth: Option<PrometheusRemoteWriteAuth>,

    #[cfg(feature = "aws-config")]
    #[configurable(derived)]
    #[configurable(metadata(docs::advanced))]
    pub aws: Option<crate::aws::RegionOrEndpoint>,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,

    #[configurable(derived)]
    #[configurable(metadata(docs::advanced))]
    #[serde(default = "default_compression")]
    #[derivative(Default(value = "default_compression()"))]
    pub compression: Compression,
}

const fn default_compression() -> Compression {
    Compression::Snappy
}

impl_generate_config_from_default!(RemoteWriteConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "prometheus_remote_write")]
impl SinkConfig for RemoteWriteConfig {
    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }

    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let endpoint = self.endpoint.parse::<Uri>().context(UriParseSnafu)?;
        let tls_settings = TlsSettings::from_options(&self.tls)?;
        let request_settings = self.request.into_settings();
        let buckets = self.buckets.clone();
        let quantiles = self.quantiles.clone();
        let default_namespace = self.default_namespace.clone();

        let client = HttpClient::new(tls_settings, cx.proxy())?;

        let auth = match &self.auth {
            Some(PrometheusRemoteWriteAuth::Basic { user, password }) => {
                Some(Auth::Basic(crate::http::Auth::Basic {
                    user: user.clone(),
                    password: password.clone().into(),
                }))
            }
            Some(PrometheusRemoteWriteAuth::Bearer { token }) => {
                Some(Auth::Basic(crate::http::Auth::Bearer {
                    token: token.clone(),
                }))
            }
            #[cfg(feature = "aws-core")]
            Some(PrometheusRemoteWriteAuth::Aws(aws_auth)) => {
                let region = self
                    .aws
                    .as_ref()
                    .map(|config| config.region())
                    .ok_or(Errors::AwsRegionRequired)?
                    .ok_or(Errors::AwsRegionRequired)?;
                Some(Auth::Aws {
                    credentials_provider: aws_auth
                        .credentials_provider(region.clone(), cx.proxy(), &self.tls)
                        .await?,
                    region,
                })
            }
            None => None,
        };

        let healthcheck = healthcheck(
            client.clone(),
            endpoint.clone(),
            self.compression,
            auth.clone(),
        )
        .boxed();

        let service = RemoteWriteService {
            endpoint,
            client,
            auth,
            compression: self.compression,
        };
        let service = ServiceBuilder::new()
            .settings(request_settings, http_response_retry_logic())
            .service(service);

        let sink = RemoteWriteSink {
            tenant_id: self.tenant_id.clone(),
            compression: self.compression,
            aggregate: self.batch.aggregate,
            batch_settings: self
                .batch
                .batch_settings
                .validate()?
                .into_batcher_settings()?,
            buckets,
            quantiles,
            default_namespace,
            service,
        };

        Ok((VectorSink::from_event_streamsink(sink), healthcheck))
    }

    fn input(&self) -> Input {
        Input::metric()
    }
}

async fn healthcheck(
    client: HttpClient,
    endpoint: Uri,
    compression: Compression,
    auth: Option<Auth>,
) -> crate::Result<()> {
    let body = bytes::Bytes::new();
    let request =
        build_request(http::Method::GET, &endpoint, compression, body, None, auth).await?;
    let response = client.send(request).await?;

    match response.status() {
        http::StatusCode::OK => Ok(()),
        other => Err(HealthcheckError::UnexpectedStatus { status: other }.into()),
    }
}
