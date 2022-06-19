// TODO: In order to correctly assert component specification compliance, we would have to do some more advanced mocking
// off the endpoint, which would include also providing a mock OAuth2 endpoint to allow for generating a token from the
// mocked credentials. Let this TODO serve as a placeholder for doing that in the future.

use bytes::Bytes;
use chrono::{DateTime, Utc};
use futures::{sink::SinkExt, FutureExt};
use goauth::scopes::Scope;
use http::Uri;
use serde::{Deserialize, Serialize};

use crate::{
    config::{AcknowledgementsConfig, Input, SinkConfig, SinkContext, SinkDescription},
    event::{Event, Metric, MetricValue},
    gcp::{GcpAuthConfig, GcpCredentials},
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

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
pub struct StackdriverConfig {
    pub project_id: String,
    pub resource: gcp::GcpTypedResource,
    #[serde(flatten)]
    pub auth: GcpAuthConfig,
    #[serde(default = "default_metric_namespace_value")]
    pub default_namespace: String,
    #[serde(default)]
    pub request: TowerRequestConfig,
    #[serde(default)]
    pub batch: BatchConfig<StackdriverMetricsDefaultBatchSettings>,
    pub tls: Option<TlsConfig>,
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

impl_generate_config_from_default!(StackdriverConfig);

inventory::submit! {
    SinkDescription::new::<StackdriverConfig>("gcp_stackdriver_metrics")
}

#[async_trait::async_trait]
#[typetag::serde(name = "gcp_stackdriver_metrics")]
impl SinkConfig for StackdriverConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let creds = self.auth.make_credentials(Scope::MonitoringWrite).await?;

        let healthcheck = healthcheck().boxed();
        let started = chrono::Utc::now();
        let request = self.request.unwrap_with(&TowerRequestConfig {
            rate_limit_num: Some(1000),
            rate_limit_duration_secs: Some(1),
            ..Default::default()
        });
        let tls_settings = TlsSettings::from_options(&self.tls)?;
        let client = HttpClient::new(tls_settings, cx.proxy())?;
        let batch_settings = self.batch.into_batch_settings()?;

        let sink = HttpEventSink {
            config: self.clone(),
            started,
            creds,
        };

        let sink = BatchedHttpSink::new(
            sink,
            MetricsBuffer::new(batch_settings.size),
            request,
            batch_settings.timeout,
            client,
            cx.acker(),
        )
        .sink_map_err(
            |error| error!(message = "Fatal gcp_stackdriver_metrics sink error.", %error),
        );

        Ok((VectorSink::from_event_sink(sink), healthcheck))
    }

    fn input(&self) -> Input {
        Input::metric()
    }

    fn sink_type(&self) -> &'static str {
        "gcp_stackdriver_metrics"
    }

    fn acknowledgements(&self) -> Option<&AcknowledgementsConfig> {
        Some(&self.acknowledgements)
    }
}

struct HttpEventSink {
    config: StackdriverConfig,
    started: DateTime<Utc>,
    creds: Option<GcpCredentials>,
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

        let end_time = data.timestamp.unwrap_or_else(chrono::Utc::now);

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
            .into_iter()
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
            "https://monitoring.googleapis.com/v3/projects/{}/timeSeries",
            self.config.project_id
        )
        .parse()?;

        let mut request = hyper::Request::post(uri)
            .header("content-type", "application/json")
            .body(body)?;

        if let Some(creds) = &self.creds {
            creds.apply(&mut request);
        }

        Ok(request)
    }
}

async fn healthcheck() -> crate::Result<()> {
    Ok(())
}
