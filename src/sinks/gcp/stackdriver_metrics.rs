use crate::{
    config::{DataType, SinkConfig, SinkContext, SinkDescription},
    event::{Event, MetricValue},
    sinks::{gcp, util::StreamSink, Healthcheck, VectorSink},
};
use chrono::{DateTime, Utc};
use futures::{stream::BoxStream, FutureExt};
use http::{header::AUTHORIZATION, HeaderValue};
use serde::{Deserialize, Serialize};
use std::{
    hash::{Hash, Hasher},
    time::Duration,
};

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct StackdriverConfig {
    pub project_id: String,
    pub resource: gcp::GcpTypedResource,
    pub credentials_path: Option<String>,
    #[serde(default = "default_metric_namespace_value")]
    pub default_namespace: String,
    pub max_batch_size: Option<usize>,
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
    async fn build(&self, _cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
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
        let sink = HttpEventSink::new(self.clone(), token);

        Ok((VectorSink::Stream(Box::new(sink)), healthcheck))
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
    client: reqwest::Client,
    token: gouth::Token,
}

impl HttpEventSink {
    fn new(config: StackdriverConfig, token: gouth::Token) -> Self {
        Self {
            config,
            token,
            started: chrono::Utc::now(),
            client: reqwest::Client::new(),
        }
    }
}

struct Wrap {
    r#type: String,
    series: gcp::GcpSerie,
}

impl PartialEq for Wrap {
    fn eq(&self, right: &Wrap) -> bool {
        self.r#type == right.r#type
    }
}

impl Eq for Wrap {}

impl Hash for Wrap {
    fn hash<H>(&self, state: &mut H)
    where
        H: Hasher,
    {
        self.r#type.hash(state)
    }
}

const DEFAULT_MAX_GCP_SERIES_BATCH_SIZE: usize = 200;
const DEFAULT_MIN_GCP_SAMPLING_PERIOD: Duration = Duration::from_secs(10);

#[async_trait::async_trait]
impl StreamSink for HttpEventSink {
    async fn run(&mut self, mut input: BoxStream<'_, Event>) -> Result<(), ()> {
        use futures::StreamExt;
        use std::time::Instant;

        let max_batch_size = self
            .config
            .max_batch_size
            .unwrap_or(DEFAULT_MAX_GCP_SERIES_BATCH_SIZE);
        let mut buffer = std::collections::HashSet::with_capacity(max_batch_size);
        let mut last_time = Instant::now();

        while let Some(event) = input.next().await {
            let metric = event.into_metric();
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

                    (value, interval, gcp::GcpMetricKind::Cumulative)
                }
                MetricValue::Gauge { value } => {
                    let interval = gcp::GcpInterval {
                        start_time: None,
                        end_time,
                    };

                    (value, interval, gcp::GcpMetricKind::Gauge)
                }

                not_supported => {
                    warn!("Unsupported metric type: {:?}.", not_supported);
                    continue;
                }
            };

            let metric_labels = series
                .tags
                .unwrap_or_default()
                .into_iter()
                .collect::<std::collections::HashMap<_, _>>();

            let series = gcp::GcpSerie {
                metric: gcp::GcpTypedResource {
                    r#type: metric_type.clone(),
                    labels: metric_labels,
                },
                resource: gcp::GcpTypedResource {
                    r#type: self.config.resource.r#type.clone(),
                    labels: self.config.resource.labels.clone(),
                },
                metric_kind,
                value_type: gcp::GcpValueType::Int64,
                points: vec![gcp::GcpPoint {
                    interval,
                    value: gcp::GcpPointValue {
                        int64_value: Some(*point_value as i64),
                    },
                }],
            };

            let wrapped = Wrap {
                r#type: metric_type,
                series,
            };

            if buffer.contains(&wrapped) {
                buffer.replace(wrapped);
            } else {
                buffer.insert(wrapped);
            }

            if buffer.len() == max_batch_size
                || last_time.elapsed() >= DEFAULT_MIN_GCP_SAMPLING_PERIOD
            {
                let time_series = buffer.drain().map(|w| w.series).collect();
                let time_series = gcp::GcpSeries { time_series };

                let uri = format!(
                    "https://monitoring.googleapis.com/v3/projects/{}/timeSeries",
                    self.config.project_id
                );

                let token_header_value = match self.token.header_value() {
                    Ok(value) => match value.parse::<HeaderValue>() {
                        Ok(value) => value,
                        Err(err) => {
                            error!(message = "Error when parsing token as an header. ", %err);
                            return Err(());
                        }
                    },

                    Err(err) => {
                        error!(message = "Error when producing a token. ", %err);
                        return Err(());
                    }
                };

                let req = self
                    .client
                    .post(uri)
                    .header("content-type", "application/json")
                    .header(AUTHORIZATION, token_header_value)
                    .json(&time_series);

                if let Err(err) = req.send().await {
                    error!(message = "Error when sending time series to GCP. ", %err);
                } else {
                    debug!("Successfully pushed time series!");
                }

                last_time = Instant::now();
            }
        }

        Ok(())
    }
}

async fn healthcheck() -> crate::Result<()> {
    Ok(())
}
