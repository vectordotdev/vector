use bytes::Bytes;
use goauth::scopes::Scope;
use http::{header::CONTENT_TYPE, Request, Uri};

use super::{
    request_builder::{StackdriverMetricsEncoder, StackdriverMetricsRequestBuilder},
    sink::StackdriverMetricsSink,
};
use crate::{
    gcp::{GcpAuthConfig, GcpAuthenticator},
    http::HttpClient,
    sinks::{
        gcp,
        prelude::*,
        util::{
            http::{
                http_response_retry_logic, HttpRequest, HttpService, HttpServiceRequestBuilder,
            },
            service::TowerRequestConfigDefaults,
        },
        HTTPRequestBuilderSnafu,
    },
};
use snafu::ResultExt;

#[derive(Clone, Copy, Debug)]
pub struct StackdriverMetricsTowerRequestConfigDefaults;

impl TowerRequestConfigDefaults for StackdriverMetricsTowerRequestConfigDefaults {
    const RATE_LIMIT_NUM: u64 = 1_000;
}

/// Configuration for the `gcp_stackdriver_metrics` sink.
#[configurable_component(sink(
    "gcp_stackdriver_metrics",
    "Deliver metrics to GCP's Cloud Monitoring system."
))]
#[derive(Clone, Debug, Default)]
pub struct StackdriverConfig {
    #[serde(skip, default = "default_endpoint")]
    pub(super) endpoint: String,

    /// The project ID to which to publish metrics.
    ///
    /// See the [Google Cloud Platform project management documentation][project_docs] for more details.
    ///
    /// [project_docs]: https://cloud.google.com/resource-manager/docs/creating-managing-projects
    pub(super) project_id: String,

    /// The monitored resource to associate the metrics with.
    pub(super) resource: gcp::GcpTypedResource,

    #[serde(flatten)]
    pub(super) auth: GcpAuthConfig,

    /// The default namespace to use for metrics that do not have one.
    ///
    /// Metrics with the same name can only be differentiated by their namespace, and not all
    /// metrics have their own namespace.
    #[serde(default = "default_metric_namespace_value")]
    pub(super) default_namespace: String,

    #[configurable(derived)]
    #[serde(default)]
    pub(super) request: TowerRequestConfig<StackdriverMetricsTowerRequestConfigDefaults>,

    #[configurable(derived)]
    #[serde(default)]
    pub(super) batch: BatchConfig<StackdriverMetricsDefaultBatchSettings>,

    #[configurable(derived)]
    pub(super) tls: Option<TlsConfig>,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub(super) acknowledgements: AcknowledgementsConfig,
}

fn default_metric_namespace_value() -> String {
    "namespace".to_string()
}

fn default_endpoint() -> String {
    "https://monitoring.googleapis.com".to_string()
}

impl_generate_config_from_default!(StackdriverConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "gcp_stackdriver_metrics")]
impl SinkConfig for StackdriverConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let auth = self.auth.build(Scope::MonitoringWrite).await?;

        let healthcheck = healthcheck().boxed();
        let started = chrono::Utc::now();
        let tls_settings = TlsSettings::from_options(&self.tls)?;
        let client = HttpClient::new(tls_settings, cx.proxy())?;

        let batch_settings = self.batch.validate()?.into_batcher_settings()?;

        let request_builder = StackdriverMetricsRequestBuilder {
            encoder: StackdriverMetricsEncoder {
                default_namespace: self.default_namespace.clone(),
                started,
                resource: self.resource.clone(),
            },
        };

        let request_limits = self.request.into_settings();

        let uri: Uri = format!(
            "{}/v3/projects/{}/timeSeries",
            self.endpoint, self.project_id
        )
        .parse()?;

        auth.spawn_regenerate_token();

        let stackdriver_metrics_service_request_builder =
            StackdriverMetricsServiceRequestBuilder { uri, auth };

        let service = HttpService::new(client, stackdriver_metrics_service_request_builder);

        let service = ServiceBuilder::new()
            .settings(request_limits, http_response_retry_logic())
            .service(service);

        let sink = StackdriverMetricsSink::new(service, batch_settings, request_builder);

        Ok((VectorSink::from_event_streamsink(sink), healthcheck))
    }

    fn input(&self) -> Input {
        Input::metric()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct StackdriverMetricsDefaultBatchSettings;

impl SinkBatchSettings for StackdriverMetricsDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = Some(1);
    const MAX_BYTES: Option<usize> = None;
    const TIMEOUT_SECS: f64 = 1.0;
}

#[derive(Debug, Clone)]
pub(super) struct StackdriverMetricsServiceRequestBuilder {
    pub(super) uri: Uri,
    pub(super) auth: GcpAuthenticator,
}

#[async_trait]
impl HttpServiceRequestBuilder<()> for StackdriverMetricsServiceRequestBuilder {
    async fn build(&self, mut request: HttpRequest<()>) -> Result<Request<Bytes>, crate::Error> {
        let builder = Request::post(self.uri.clone()).header(CONTENT_TYPE, "application/json");

        let mut request = builder
            .body(request.take_payload())
            .context(HTTPRequestBuilderSnafu)
            .map_err(Into::<crate::Error>::into)?;

        self.auth.apply(&mut request);

        Ok(request)
    }
}

async fn healthcheck() -> crate::Result<()> {
    Ok(())
}
