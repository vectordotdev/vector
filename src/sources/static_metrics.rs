use std::collections::BTreeMap;
use std::num::NonZeroU32;
use std::time::Duration;

use chrono::Utc;
use futures::StreamExt;
use serde_with::serde_as;
use tokio::time;
use tokio_stream::wrappers::IntervalStream;
use vector_lib::configurable::configurable_component;
use vector_lib::internal_event::{
    ByteSize, BytesReceived, CountByteSize, InternalEventHandle as _, Protocol,
};
use vector_lib::{config::LogNamespace, ByteSizeOf, EstimatedJsonEncodedSizeOf};

use crate::{
    config::{SourceConfig, SourceContext, SourceOutput},
    event::{
        metric::{MetricData, MetricName, MetricSeries, MetricTime, MetricValue},
        EventMetadata, Metric, MetricKind,
    },
    internal_events::{EventsReceived, StreamClosedError},
    shutdown::ShutdownSignal,
    SourceSender,
};

/// Configuration for the `static_metrics` source.
#[serde_as]
#[configurable_component(source(
    "static_metrics",
    "Produce static metrics defined in configuration."
))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct StaticMetricsConfig {
    /// The interval between metric emitting, in seconds.
    #[serde_as(as = "serde_with::DurationSecondsWithFrac<f64>")]
    #[serde(default = "default_interval")]
    #[configurable(metadata(docs::human_name = "Emitting interval"))]
    pub interval_secs: Duration,

    /// Overrides the default namespace for the metrics emitted by the source.
    #[serde(default = "default_namespace")]
    pub namespace: String,

    #[configurable(derived)]
    #[serde(default)]
    pub metrics: Vec<StaticMetricConfig>,
}

impl Default for StaticMetricsConfig {
    fn default() -> Self {
        Self {
            interval_secs: default_interval(),
            metrics: Vec::default(),
            namespace: default_namespace(),
        }
    }
}

/// Tag configuration for the `internal_metrics` source.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct StaticMetricConfig {
    /// Name of the static metric
    pub name: String,

    /// "Observed" value of the static metric
    pub value: MetricValue,

    /// Kind of the static metric - either absolute or incremental
    pub kind: MetricKind,

    /// Key-value pairs representing tags and their values to add to the metric.
    #[configurable(metadata(
        docs::additional_props_description = "An individual tag - value pair."
    ))]
    pub tags: BTreeMap<String, String>,
}

fn default_interval() -> Duration {
    Duration::from_secs_f64(1.0)
}

fn default_namespace() -> String {
    "static".to_owned()
}

impl_generate_config_from_default!(StaticMetricsConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "static_metrics")]
impl SourceConfig for StaticMetricsConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        if self.interval_secs.is_zero() {
            warn!(
                "Interval set to 0 secs, this could result in high CPU utilization. It is suggested to use interval >= 1 secs.",
            );
        }
        let interval = self.interval_secs;

        let namespace = self.namespace.clone();

        let metrics = self.metrics.clone();

        Ok(Box::pin(
            StaticMetrics {
                namespace,
                metrics,
                interval,
                out: cx.out,
                shutdown: cx.shutdown,
            }
            .run(),
        ))
    }

    fn outputs(&self, _global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        vec![SourceOutput::new_metrics()]
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}

struct StaticMetrics {
    namespace: String,
    metrics: Vec<StaticMetricConfig>,
    interval: time::Duration,
    out: SourceSender,
    shutdown: ShutdownSignal,
}

impl StaticMetrics {
    async fn run(mut self) -> Result<(), ()> {
        let events_received = register!(EventsReceived);
        let bytes_received = register!(BytesReceived::from(Protocol::STATIC));
        let mut interval =
            IntervalStream::new(time::interval(self.interval)).take_until(self.shutdown);

        // Prepare metrics, since they are static and won't change
        let metrics: Vec<Metric> = self
            .metrics
            .into_iter()
            .map(
                |StaticMetricConfig {
                     name,
                     value,
                     kind,
                     tags,
                 }| {
                    Metric::from_parts(
                        MetricSeries {
                            name: MetricName {
                                name,
                                namespace: Some(self.namespace.clone()),
                            },
                            tags: Some(tags.into()),
                        },
                        MetricData {
                            time: MetricTime {
                                timestamp: None,
                                interval_ms: NonZeroU32::new(self.interval.as_millis() as u32),
                            },
                            kind,
                            value: value.clone(),
                        },
                        EventMetadata::default(),
                    )
                },
            )
            .collect();

        while interval.next().await.is_some() {
            let count = metrics.len();
            let byte_size = metrics.size_of();
            let json_size = metrics.estimated_json_encoded_size_of();

            bytes_received.emit(ByteSize(byte_size));
            events_received.emit(CountByteSize(count, json_size));

            let batch = metrics
                .clone()
                .into_iter()
                .map(|metric| metric.with_timestamp(Some(Utc::now())));

            if (self.out.send_batch(batch).await).is_err() {
                emit!(StreamClosedError { count });
                return Err(());
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        event::Event,
        test_util::{
            self,
            components::{run_and_assert_source_compliance, SOURCE_TAGS},
        },
    };

    #[test]
    fn generate_config() {
        test_util::test_generate_config::<StaticMetricsConfig>();
    }

    async fn events_from_config(config: StaticMetricsConfig) -> Vec<Event> {
        run_and_assert_source_compliance(config, time::Duration::from_millis(100), &SOURCE_TAGS)
            .await
    }

    fn default_metric() -> StaticMetricConfig {
        StaticMetricConfig {
            name: "".to_string(),
            value: MetricValue::Gauge { value: 0.0 },
            kind: MetricKind::Absolute,
            tags: BTreeMap::default(),
        }
    }

    #[tokio::test]
    async fn default_empty() {
        let events = events_from_config(StaticMetricsConfig::default()).await;

        assert!(events.is_empty());
    }

    #[tokio::test]
    async fn default_namespace() {
        let mut events = events_from_config(StaticMetricsConfig {
            metrics: vec![default_metric()],
            ..Default::default()
        })
        .await;

        assert!(!events.is_empty());
        let event = events.remove(0);
        assert_eq!(event.as_metric().namespace(), Some("static"));
    }

    #[tokio::test]
    async fn default_namespace_multiple_events() {
        let mut events = events_from_config(StaticMetricsConfig {
            metrics: vec![default_metric(), default_metric()],
            ..Default::default()
        })
        .await;

        assert!(!events.is_empty());
        let event = events.remove(0);
        assert_eq!(event.as_metric().namespace(), Some("static"));
        let event = events.remove(0);
        assert_eq!(event.as_metric().namespace(), Some("static"));
    }

    #[tokio::test]
    async fn namespace() {
        let namespace = "totally_custom";

        let config = StaticMetricsConfig {
            namespace: namespace.to_owned(),
            metrics: vec![default_metric()],
            ..StaticMetricsConfig::default()
        };

        let mut events = events_from_config(config).await;
        assert!(!events.is_empty());
        let event = events.remove(0);

        assert_eq!(event.as_metric().namespace(), Some(namespace));
    }

    #[tokio::test]
    async fn sets_custom_tags() {
        let mut events = events_from_config(StaticMetricsConfig {
            metrics: vec![StaticMetricConfig {
                name: "test".to_string(),
                value: MetricValue::Gauge { value: 2.3 },
                kind: MetricKind::Absolute,
                tags: BTreeMap::from([("custom_tag".to_string(), "custom_tag_value".to_string())]),
            }],
            ..Default::default()
        })
        .await;

        assert!(!events.is_empty());
        let event = events.remove(0);
        let metric = event.as_metric();

        assert_eq!(metric.name(), "test");
        assert!(matches!(metric.value(), MetricValue::Gauge { value: 2.3 }));
        assert_eq!(
            metric.tag_value("custom_tag"),
            Some("custom_tag_value".to_string())
        );
    }
}
