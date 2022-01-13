use std::num::NonZeroU64;

use chrono::{DateTime, Utc};
use futures::{sink::SinkExt, FutureExt};
use http::{header::AUTHORIZATION, HeaderValue, Uri};
use serde::{Deserialize, Serialize};

use crate::{
    config::{DataType, SinkConfig, SinkContext, SinkDescription},
    event::{Event, Metric, MetricValue},
    http::HttpClient,
    sinks::{
        gcp,
        util::{
            buffer::metrics::MetricsBuffer,
            http::{BatchedHttpSink, HttpSink},
            BatchConfig, SinkBatchSettings, TowerRequestConfig,
        },
        Healthcheck, VectorSink,
    },
    tls::{TlsOptions, TlsSettings},
};

#[derive(Clone, Copy, Debug, Default)]
pub struct StackdriverMetricsDefaultBatchSettings;

impl SinkBatchSettings for StackdriverMetricsDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = Some(1);
    const MAX_BYTES: Option<usize> = None;
    const TIMEOUT_SECS: NonZeroU64 = unsafe { NonZeroU64::new_unchecked(1) };
}

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct StackdriverConfig {
    pub project_id: String,
    pub resource: gcp::GcpTypedResource,
    pub credentials_path: Option<String>,
    #[serde(default = "default_metric_namespace_value")]
    pub default_namespace: String,
    #[serde(default)]
    pub request: TowerRequestConfig,
    #[serde(default)]
    pub batch: BatchConfig<StackdriverMetricsDefaultBatchSettings>,
    pub tls: Option<TlsOptions>,
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
        let mut token = gouth::Builder::new().scopes(&[
            "https://www.googleapis.com/auth/cloud-platform",
            "https://www.googleapis.com/auth/monitoring",
            "https://www.googleapis.com/auth/monitoring.write",
        ]);

        if let Some(credentials_path) = self.credentials_path.as_ref() {
            token = token.file(credentials_path);
        }

        let token = token.build()?;
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
            token,
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

    fn input_type(&self) -> DataType {
        DataType::Metric
    }

    fn sink_type(&self) -> &'static str {
        "gcp_stackdriver_metrics"
    }
}

struct HttpEventSink {
    config: StackdriverConfig,
    started: DateTime<Utc>,
    token: gouth::Token,
}

#[async_trait::async_trait]
impl HttpSink for HttpEventSink {
    type Input = Metric;
    type Output = Vec<Metric>;

    fn encode_event(&self, event: Event) -> Option<Self::Input> {
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

    async fn build_request(
        &self,
        mut metrics: Self::Output,
    ) -> crate::Result<hyper::Request<Vec<u8>>> {
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

        let body = serde_json::to_vec(&series).unwrap();
        let uri: Uri = format!(
            "https://monitoring.googleapis.com/v3/projects/{}/timeSeries",
            self.config.project_id
        )
        .parse()?;

        let request = hyper::Request::post(uri)
            .header("content-type", "application/json")
            .header(
                AUTHORIZATION,
                self.token.header_value()?.parse::<HeaderValue>()?,
            )
            .body(body)?;

        Ok(request)
    }
}

async fn healthcheck() -> crate::Result<()> {
    Ok(())
}
