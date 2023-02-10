// TODO: In order to correctly assert component specification compliance, we would have to do some more advanced mocking
// off the endpoint, which would include also providing a mock OAuth2 endpoint to allow for generating a token from the
// mocked credentials. Let this TODO serve as a placeholder for doing that in the future.

use bytes::Bytes;
use chrono::{DateTime, Utc};
use futures::{sink::SinkExt, FutureExt};
use goauth::scopes::Scope;
use http::Uri;
use vector_config::configurable_component;

use crate::{
    config::{AcknowledgementsConfig, Input, SinkConfig, SinkContext},
    event::{Event, Metric, MetricValue},
    gcp::{GcpAuthConfig, GcpAuthenticator},
    http::HttpClient,
    sinks::{
        gcp,
        util::{
            buffer::metrics::MetricsBuffer,
            http::{BatchedHttpSink, HttpEventEncoder, HttpSink},
            BatchConfig, SinkBatchSettings, TowerRequestConfig,
        },
        Healthcheck, VectorSink,
    },
    tls::{TlsConfig, TlsSettings},
};

#[derive(Clone, Copy, Debug, Default)]
pub struct StackdriverMetricsDefaultBatchSettings;

impl SinkBatchSettings for StackdriverMetricsDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = Some(1);
    const MAX_BYTES: Option<usize> = None;
    const TIMEOUT_SECS: f64 = 1.0;
}

/// Configuration for the `gcp_stackdriver_metrics` sink.
#[configurable_component(sink("gcp_stackdriver_metrics"))]
#[derive(Clone, Debug, Default)]
pub struct StackdriverConfig {
    #[serde(skip, default = "default_endpoint")]
    endpoint: String,

    /// The project ID to which to publish metrics.
    ///
    /// See the [Google Cloud Platform project management documentation][project_docs] for more details.
    ///
    /// [project_docs]: https://cloud.google.com/resource-manager/docs/creating-managing-projects
    pub project_id: String,

    /// The monitored resource to associate the metrics with.
    pub resource: gcp::GcpTypedResource,

    #[serde(flatten)]
    pub auth: GcpAuthConfig,

    /// The default namespace to use for metrics that do not have one.
    ///
    /// Metrics with the same name can only be differentiated by their namespace, and not all
    /// metrics have their own namespace.
    #[serde(default = "default_metric_namespace_value")]
    pub default_namespace: String,

    #[configurable(derived)]
    #[serde(default)]
    pub request: TowerRequestConfig,

    #[configurable(derived)]
    #[serde(default)]
    pub batch: BatchConfig<StackdriverMetricsDefaultBatchSettings>,

    #[configurable(derived)]
    pub tls: Option<TlsConfig>,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    acknowledgements: AcknowledgementsConfig,
}

fn default_metric_namespace_value() -> String {
    "namespace".to_string()
}

fn default_endpoint() -> String {
    "https://monitoring.googleapis.com".to_string()
}

impl_generate_config_from_default!(StackdriverConfig);

#[async_trait::async_trait]
impl SinkConfig for StackdriverConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let auth = self.auth.build(Scope::MonitoringWrite).await?;

        let healthcheck = healthcheck().boxed();
        let started = chrono::Utc::now();
        let request = self.request.unwrap_with(
            &TowerRequestConfig::default()
                .rate_limit_duration_secs(1)
                .rate_limit_num(1000),
        );
        let tls_settings = TlsSettings::from_options(&self.tls)?;
        let client = HttpClient::new(tls_settings, cx.proxy())?;
        let batch_settings = self.batch.into_batch_settings()?;

        let sink = HttpEventSink {
            config: self.clone(),
            started,
            auth,
        };

        let sink = BatchedHttpSink::new(
            sink,
            MetricsBuffer::new(batch_settings.size),
            request,
            batch_settings.timeout,
            client,
        )
        .sink_map_err(
            |error| error!(message = "Fatal gcp_stackdriver_metrics sink error.", %error),
        );

        Ok((VectorSink::from_event_sink(sink), healthcheck))
    }

    fn input(&self) -> Input {
        Input::metric()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

struct HttpEventSink {
    config: StackdriverConfig,
    started: DateTime<Utc>,
    auth: GcpAuthenticator,
}

struct StackdriverMetricsEncoder;

impl HttpEventEncoder<Metric> for StackdriverMetricsEncoder {
    fn encode_event(&mut self, event: Event) -> Option<Metric> {
        let metric = event.into_metric();

        match metric.value() {
            &MetricValue::Counter { .. } => Some(metric),
            &MetricValue::Gauge { .. } => Some(metric),
            not_supported => {
                warn!("Unsupported metric type: {:?}.", not_supported);
                None
            }
        }
    }
}

#[async_trait::async_trait]
impl HttpSink for HttpEventSink {
    type Input = Metric;
    type Output = Vec<Metric>;
    type Encoder = StackdriverMetricsEncoder;

    fn build_encoder(&self) -> Self::Encoder {
        StackdriverMetricsEncoder
    }

    async fn build_request(
        &self,
        mut metrics: Self::Output,
    ) -> crate::Result<hyper::Request<Bytes>> {
        let metric = metrics.pop().expect("only one metric");
        let (series, data, _metadata) = metric.into_parts();
        let namespace = series
            .name
            .namespace
            .unwrap_or_else(|| self.config.default_namespace.clone());
        let metric_type = format!(
            "custom.googleapis.com/{}/metrics/{}",
            namespace, series.name.name
        );

        let end_time = data.time.timestamp.unwrap_or_else(chrono::Utc::now);

        let (point_value, interval, metric_kind) = match &data.value {
            MetricValue::Counter { value } => {
                let interval = gcp::GcpInterval {
                    start_time: Some(self.started),
                    end_time,
                };

                (*value, interval, gcp::GcpMetricKind::Cumulative)
            }
            MetricValue::Gauge { value } => {
                let interval = gcp::GcpInterval {
                    start_time: None,
                    end_time,
                };

                (*value, interval, gcp::GcpMetricKind::Gauge)
            }
            _ => unreachable!(),
        };

        let metric_labels = series
            .tags
            .unwrap_or_default()
            .into_iter_single()
            .collect::<std::collections::HashMap<_, _>>();

        let series = gcp::GcpSeries {
            time_series: &[gcp::GcpSerie {
                metric: gcp::GcpTypedResource {
                    r#type: metric_type,
                    labels: metric_labels,
                },
                resource: gcp::GcpTypedResource {
                    r#type: self.config.resource.r#type.clone(),
                    labels: self.config.resource.labels.clone(),
                },
                metric_kind,
                value_type: gcp::GcpValueType::Int64,
                points: &[gcp::GcpPoint {
                    interval,
                    value: gcp::GcpPointValue {
                        int64_value: Some(point_value as i64),
                    },
                }],
            }],
        };

        let body = crate::serde::json::to_bytes(&series).unwrap().freeze();

        let uri: Uri = format!(
            "{}/v3/projects/{}/timeSeries",
            self.config.endpoint, self.config.project_id
        )
        .parse()?;

        let mut request = hyper::Request::post(uri)
            .header("content-type", "application/json")
            .body(body)?;
        self.auth.apply(&mut request);

        Ok(request)
    }
}

async fn healthcheck() -> crate::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use futures::{future::ready, stream};
    use serde::Deserialize;
    use vector_core::event::{MetricKind, MetricValue};

    use super::*;
    use crate::{
        config::{GenerateConfig, SinkConfig, SinkContext},
        test_util::{
            components::{run_and_assert_sink_compliance, SINK_TAGS},
            http::{always_200_response, spawn_blackhole_http_server},
        },
    };

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<StackdriverConfig>();
    }

    #[tokio::test]
    async fn component_spec_compliance() {
        let mock_endpoint = spawn_blackhole_http_server(always_200_response).await;

        let config = StackdriverConfig::generate_config().to_string();
        let mut config = StackdriverConfig::deserialize(toml::de::ValueDeserializer::new(&config))
            .expect("config should be valid");

        // If we don't override the credentials path/API key, it tries to directly call out to the Google Instance
        // Metadata API, which we clearly don't have in unit tests. :)
        config.auth.credentials_path = None;
        config.auth.api_key = Some("fake".to_string().into());
        config.endpoint = mock_endpoint.to_string();

        let context = SinkContext::new_test();
        let (sink, _healthcheck) = config.build(context).await.unwrap();

        let event = Event::Metric(Metric::new(
            "gauge-test",
            MetricKind::Absolute,
            MetricValue::Gauge { value: 1_f64 },
        ));
        run_and_assert_sink_compliance(sink, stream::once(ready(event)), &SINK_TAGS).await;
    }
}
