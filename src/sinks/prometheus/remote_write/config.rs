use http::Uri;
use snafu::prelude::*;

use crate::{
    aws::RegionOrEndpoint,
    http::{Auth, HttpClient},
    sinks::{prelude::*, prometheus::PrometheusRemoteWriteAuth},
};

use super::{
    request_builder::build_request,
    sink::{PrometheusRemoteWriteDefaultBatchSettings, RemoteWriteSink},
    Errors,
};

/// Configuration for the `prometheus_remote_write` sink.
#[configurable_component(sink(
    "prometheus_remote_write",
    "Deliver metric data to a Prometheus remote write endpoint."
))]
#[derive(Clone, Debug, Default)]
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
    pub batch: BatchConfig<PrometheusRemoteWriteDefaultBatchSettings>,

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

    #[configurable(derived)]
    #[configurable(metadata(docs::advanced))]
    pub aws: Option<RegionOrEndpoint>,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,

    #[configurable(derived)]
    #[configurable(metadata(docs::advanced))]
    #[serde(default)]
    pub compression: super::Compression,
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
        let batch = self.batch.into_batch_settings()?;
        let request_settings = self.request.unwrap_with(&TowerRequestConfig::default());
        let buckets = self.buckets.clone();
        let quantiles = self.quantiles.clone();

        let client = HttpClient::new(tls_settings, cx.proxy())?;
        let tenant_id = self.tenant_id.clone();

        let (http_auth, credentials_provider, aws_region) = match &self.auth {
            Some(PrometheusRemoteWriteAuth::Basic { user, password }) => (
                Some(Auth::Basic {
                    user: user.clone(),
                    password: password.clone().into(),
                }),
                None,
                None,
            ),
            Some(PrometheusRemoteWriteAuth::Bearer { token }) => (
                Some(Auth::Bearer {
                    token: token.clone(),
                }),
                None,
                None,
            ),
            Some(PrometheusRemoteWriteAuth::Aws(aws_auth)) => {
                let region = self
                    .aws
                    .as_ref()
                    .map(|config| config.region())
                    .ok_or(Errors::AwsRegionRequired)?
                    .ok_or(Errors::AwsRegionRequired)?;

                (
                    None,
                    Some(aws_auth.credentials_provider(region.clone()).await?),
                    Some(region),
                )
            }
            None => (None, None, None),
        };

        // let http_request_builder = Arc::new(RemoteWriteRequestBuilder {
        //     endpoint: endpoint.clone(),
        //     aws_region,
        //     credentials_provider,
        //     http_auth,
        //     compression: self.compression,
        // });

        let healthcheck = healthcheck(
            client.clone(),
            endpoint,
            self.compression.clone(),
            http_auth.clone(),
        )
        .boxed();

        let sink = RemoteWriteSink {
            tenant_id: self.tenant_id.clone(),
            compression: self.compression.clone(),
            batch_settings: self.batch.validate()?.into_batcher_settings()?,
            endpoint: endpoint.clone(),
            http_auth,
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
    compression: super::Compression,
    auth: Option<Auth>,
) -> crate::Result<()> {
    let body = bytes::Bytes::new();
    // let request = http_request_builder.do_the_thing(http::Method::GET, body.into(), None);
    let request = build_request(
        http::Method::GET,
        endpoint,
        compression,
        auth,
        body.into(),
        None,
    );
    // TODO Sign the request
    let response = client.send(request).await?;

    match response.status() {
        http::StatusCode::OK => Ok(()),
        other => Err(HealthcheckError::UnexpectedStatus { status: other }.into()),
    }
}
