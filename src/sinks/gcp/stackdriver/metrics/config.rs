use bytes::Bytes;
use goauth::scopes::Scope;
use http::{Request, Uri, header::CONTENT_TYPE};
use snafu::ResultExt;

use super::{
    request_builder::{StackdriverMetricsEncoder, StackdriverMetricsRequestBuilder},
    sink::StackdriverMetricsSink,
};
use crate::{
    config::ValidatedSink,
    gcp::{GcpAuthConfig, GcpAuthenticator},
    http::HttpClient,
    sinks::{
        HTTPRequestBuilderSnafu, gcp,
        prelude::*,
        util::{
            HttpEndpoint,
            http::{
                HttpRequest, HttpService, HttpServiceRequestBuilder, RetryStrategy,
                http_response_retry_logic,
            },
            service::TowerRequestConfigDefaults,
        },
    },
};

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
#[derive(Clone, Debug, Derivative)]
#[derivative(Default)]
pub struct StackdriverConfig {
    #[derivative(Default(value = "default_endpoint()"))]
    #[serde(skip, default = "default_endpoint")]
    pub(super) endpoint: HttpEndpoint,

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

    #[serde(default)]
    pub(super) request: TowerRequestConfig<StackdriverMetricsTowerRequestConfigDefaults>,

    #[serde(default)]
    pub(super) batch: BatchConfig<StackdriverMetricsDefaultBatchSettings>,

    pub(super) tls: Option<TlsConfig>,

    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub(super) acknowledgements: AcknowledgementsConfig,

    #[serde(default)]
    pub retry_strategy: RetryStrategy,
}

fn default_metric_namespace_value() -> String {
    "namespace".to_string()
}

fn default_endpoint() -> HttpEndpoint {
    HttpEndpoint::parse("https://monitoring.googleapis.com")
        .expect("static default endpoint should be a valid http(s) URL")
}

impl_generate_config_from_default!(StackdriverConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "gcp_stackdriver_metrics")]
impl SinkConfig for StackdriverConfig {
    fn input(&self) -> Input {
        Input::metric()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

#[async_trait::async_trait]
impl ValidatedSink for StackdriverConfig {
    type Validated = ValidatedStackdriverMetrics;

    fn validate(&self) -> crate::Result<ValidatedStackdriverMetrics> {
        let batch_settings = self.batch.validate()?.into_batcher_settings()?;

        let uri = self
            .endpoint
            .append_path(&format!("/v3/projects/{}/timeSeries", self.project_id))?;

        Ok(ValidatedStackdriverMetrics {
            batch_settings,
            uri,
        })
    }

    async fn build(
        &self,
        validated: &ValidatedStackdriverMetrics,
        cx: SinkContext,
    ) -> crate::Result<(VectorSink, Healthcheck)> {
        let ValidatedStackdriverMetrics {
            batch_settings,
            uri,
        } = validated.clone();

        let auth = self.auth.build(Scope::MonitoringWrite).await?;

        let healthcheck = healthcheck().boxed();
        let started = chrono::Utc::now();
        let tls_settings = TlsSettings::from_options(self.tls.as_ref())?;
        let client = HttpClient::new(tls_settings, cx.proxy())?;

        let request_builder = StackdriverMetricsRequestBuilder {
            encoder: StackdriverMetricsEncoder {
                default_namespace: self.default_namespace.clone(),
                started,
                resource: self.resource.clone(),
            },
        };

        let request_limits = self.request.into_settings();

        auth.spawn_regenerate_token();

        let stackdriver_metrics_service_request_builder = StackdriverMetricsServiceRequestBuilder {
            uri: uri.into_uri(),
            auth,
        };

        let service = HttpService::new(client, stackdriver_metrics_service_request_builder);

        let service = ServiceBuilder::new()
            .settings(
                request_limits,
                http_response_retry_logic(self.retry_strategy.clone()),
            )
            .service(service);

        let sink = StackdriverMetricsSink::new(service, batch_settings, request_builder);

        Ok((VectorSink::from_event_streamsink(sink), healthcheck))
    }
}

#[derive(Clone, Debug)]
pub struct ValidatedStackdriverMetrics {
    batch_settings: BatcherSettings,
    uri: HttpEndpoint,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ValidatedSink;

    #[test]
    fn validate_produces_usable_values() {
        let config = StackdriverConfig {
            project_id: "test-project".into(),
            endpoint: default_endpoint(),
            ..Default::default()
        };
        let validated = config.validate().expect("validation should succeed");
        assert_eq!(
            validated.uri.to_string(),
            "https://monitoring.googleapis.com/v3/projects/test-project/timeSeries"
        );
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

impl HttpServiceRequestBuilder<()> for StackdriverMetricsServiceRequestBuilder {
    fn build(&self, mut request: HttpRequest<()>) -> Result<Request<Bytes>, crate::Error> {
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
