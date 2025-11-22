use std::time::Duration;

use chrono::Utc;
use futures::StreamExt;
use serde_with::serde_as;
use tokio::time;
use tokio_stream::wrappers::IntervalStream;
use vector_lib::{
    ByteSizeOf, EstimatedJsonEncodedSizeOf,
    config::LogNamespace,
    configurable::configurable_component,
    internal_event::{ByteSize, BytesReceived, CountByteSize, InternalEventHandle as _, Protocol},
    lookup::lookup_v2::OptionalValuePath,
};

use crate::{
    SourceSender,
    config::{SharedTopologyMetadata, SourceConfig, SourceContext, SourceOutput, log_schema},
    event::{Metric, MetricKind, MetricTags, MetricValue},
    internal_events::{EventsReceived, StreamClosedError},
    metrics::Controller,
    shutdown::ShutdownSignal,
};

/// Configuration for the `internal_metrics` source.
#[serde_as]
#[configurable_component(source(
    "internal_metrics",
    "Expose internal metrics emitted by the running Vector instance."
))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields, default)]
pub struct InternalMetricsConfig {
    /// The interval between metric gathering, in seconds.
    #[serde_as(as = "serde_with::DurationSecondsWithFrac<f64>")]
    #[serde(default = "default_scrape_interval")]
    #[configurable(metadata(docs::human_name = "Scrape Interval"))]
    pub scrape_interval_secs: Duration,

    #[configurable(derived)]
    pub tags: TagsConfig,

    /// Overrides the default namespace for the metrics emitted by the source.
    #[serde(default = "default_namespace")]
    pub namespace: String,
}

impl Default for InternalMetricsConfig {
    fn default() -> Self {
        Self {
            scrape_interval_secs: default_scrape_interval(),
            tags: TagsConfig::default(),
            namespace: default_namespace(),
        }
    }
}

/// Tag configuration for the `internal_metrics` source.
#[configurable_component]
#[derive(Clone, Debug, Default)]
#[serde(deny_unknown_fields, default)]
pub struct TagsConfig {
    /// Overrides the name of the tag used to add the peer host to each metric.
    ///
    /// The value is the peer host's address, including the port. For example, `1.2.3.4:9000`.
    ///
    /// By default, the [global `log_schema.host_key` option][global_host_key] is used.
    ///
    /// Set to `""` to suppress this key.
    ///
    /// [global_host_key]: https://vector.dev/docs/reference/configuration/global-options/#log_schema.host_key
    pub host_key: Option<OptionalValuePath>,

    /// Sets the name of the tag to use to add the current process ID to each metric.
    ///
    ///
    /// By default, this is not set and the tag is not automatically added.
    #[configurable(metadata(docs::examples = "pid"))]
    pub pid_key: Option<String>,
}

fn default_scrape_interval() -> Duration {
    Duration::from_secs_f64(1.0)
}

fn default_namespace() -> String {
    "vector".to_owned()
}

impl_generate_config_from_default!(InternalMetricsConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "internal_metrics")]
impl SourceConfig for InternalMetricsConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        if self.scrape_interval_secs.is_zero() {
            warn!(
                "Interval set to 0 secs, this could result in high CPU utilization. It is suggested to use interval >= 1 secs.",
            );
        }
        let interval = self.scrape_interval_secs;

        // namespace for created metrics is already "vector" by default.
        let namespace = self.namespace.clone();

        let host_key = self
            .tags
            .host_key
            .clone()
            .unwrap_or(log_schema().host_key().cloned().into());

        let pid_key = self
            .tags
            .pid_key
            .as_deref()
            .and_then(|tag| (!tag.is_empty()).then(|| tag.to_owned()));

        Ok(Box::pin(
            InternalMetrics {
                namespace,
                host_key,
                pid_key,
                controller: Controller::get()?,
                interval,
                out: cx.out,
                shutdown: cx.shutdown,
                topology_metadata: cx.topology_metadata.clone(),
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

struct InternalMetrics<'a> {
    namespace: String,
    host_key: OptionalValuePath,
    pid_key: Option<String>,
    controller: &'a Controller,
    interval: time::Duration,
    out: SourceSender,
    shutdown: ShutdownSignal,
    topology_metadata: Option<SharedTopologyMetadata>,
}

impl InternalMetrics<'_> {
    async fn run(mut self) -> Result<(), ()> {
        let events_received = register!(EventsReceived);
        let bytes_received = register!(BytesReceived::from(Protocol::INTERNAL));
        let mut interval =
            IntervalStream::new(time::interval(self.interval)).take_until(self.shutdown);
        let namespace_override = (self.namespace != "vector").then(|| self.namespace.clone());
        let host_tag_key = self.host_key.path.as_ref().map(|path| path.to_string());
        let pid_tag_key = self.pid_key.clone();
        while interval.next().await.is_some() {
            let hostname = crate::get_hostname().ok();
            let pid = std::process::id().to_string();

            let metrics = self.controller.capture_metrics();
            let count = metrics.len();
            let byte_size = metrics.size_of();
            let json_size = metrics.estimated_json_encoded_size_of();

            bytes_received.emit(ByteSize(byte_size));
            events_received.emit(CountByteSize(count, json_size));

            let mut batch: Vec<Metric> = metrics
                .into_iter()
                .map(|metric| {
                    apply_common_tags(
                        metric,
                        namespace_override.as_ref(),
                        host_tag_key.as_ref(),
                        hostname.as_ref(),
                        pid_tag_key.as_ref(),
                        &pid,
                    )
                })
                .collect();

            // Add topology metrics if available
            if let Some(topology_metadata) = &self.topology_metadata {
                let topology = topology_metadata.read().unwrap();
                let topology_metrics = generate_topology_metrics(&topology, Utc::now());
                batch.extend(topology_metrics.into_iter().map(|metric| {
                    apply_common_tags(
                        metric,
                        namespace_override.as_ref(),
                        host_tag_key.as_ref(),
                        hostname.as_ref(),
                        pid_tag_key.as_ref(),
                        &pid,
                    )
                }));
            }

            if (self.out.send_batch(batch.into_iter()).await).is_err() {
                emit!(StreamClosedError { count });
                return Err(());
            }
        }

        Ok(())
    }
}

fn apply_common_tags(
    mut metric: Metric,
    namespace_override: Option<&String>,
    host_tag_key: Option<&String>,
    hostname: Option<&String>,
    pid_tag_key: Option<&String>,
    pid: &str,
) -> Metric {
    if let Some(namespace) = namespace_override {
        metric = metric.with_namespace(Some(namespace.clone()));
    }

    if let (Some(key), Some(host)) = (host_tag_key, hostname) {
        metric.replace_tag(key.clone(), host.clone());
    }

    if let Some(pid_key) = pid_tag_key {
        metric.replace_tag(pid_key.clone(), pid.to_owned());
    }

    metric
}

/// Generate metrics for topology connections
fn generate_topology_metrics(
    topology: &crate::config::TopologyMetadata,
    timestamp: chrono::DateTime<Utc>,
) -> Vec<Metric> {
    let mut metrics = Vec::new();

    for (to_component, inputs) in &topology.inputs {
        for input in inputs {
            let mut tags = MetricTags::default();

            // Source component labels
            tags.insert("from_component_id".to_string(), input.component.to_string());
            if let Some((type_name, kind)) = topology.component_types.get(&input.component) {
                tags.insert("from_component_type".to_string(), type_name.clone());
                tags.insert("from_component_kind".to_string(), kind.clone());
            }
            if let Some(port) = &input.port {
                tags.insert("from_output".to_string(), port.clone());
            }

            // Target component labels
            tags.insert("to_component_id".to_string(), to_component.to_string());
            if let Some((type_name, kind)) = topology.component_types.get(to_component) {
                tags.insert("to_component_type".to_string(), type_name.clone());
                tags.insert("to_component_kind".to_string(), kind.clone());
            }

            metrics.push(
                Metric::new(
                    "component_connections",
                    MetricKind::Absolute,
                    MetricValue::Gauge { value: 1.0 },
                )
                .with_namespace(Some("vector".to_string()))
                .with_tags(Some(tags))
                .with_timestamp(Some(timestamp)),
            );
        }
    }

    metrics
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use metrics::{counter, gauge, histogram};
    use vector_lib::{metric_tags, metrics::Controller};

    use super::*;
    use crate::{
        config::{ComponentKey, OutputId},
        event::{
            Event,
            metric::{Metric, MetricValue},
        },
        test_util::{
            self,
            components::{SOURCE_TAGS, run_and_assert_source_compliance},
        },
    };

    #[test]
    fn generate_config() {
        test_util::test_generate_config::<InternalMetricsConfig>();
    }

    #[test]
    fn captures_internal_metrics() {
        test_util::trace_init();

        // There *seems* to be a race condition here (CI was flaky), so add a slight delay.
        std::thread::sleep(std::time::Duration::from_millis(300));

        gauge!("foo").set(1.0);
        gauge!("foo").set(2.0);
        counter!("bar").increment(3);
        counter!("bar").increment(4);
        histogram!("baz").record(5.0);
        histogram!("baz").record(6.0);
        histogram!("quux", "host" => "foo").record(8.0);
        histogram!("quux", "host" => "foo").record(8.1);

        let controller = Controller::get().expect("no controller");

        // There *seems* to be a race condition here (CI was flaky), so add a slight delay.
        std::thread::sleep(std::time::Duration::from_millis(300));

        let output = controller
            .capture_metrics()
            .into_iter()
            .map(|metric| (metric.name().to_string(), metric))
            .collect::<BTreeMap<String, Metric>>();

        assert_eq!(&MetricValue::Gauge { value: 2.0 }, output["foo"].value());
        assert_eq!(&MetricValue::Counter { value: 7.0 }, output["bar"].value());

        match &output["baz"].value() {
            MetricValue::AggregatedHistogram {
                buckets,
                count,
                sum,
            } => {
                // This index is _only_ stable so long as the offsets in
                // [`metrics::handle::Histogram::new`] are hard-coded. If this
                // check fails you might look there and see if we've allowed
                // users to set their own bucket widths.
                assert_eq!(buckets[9].count, 2);
                assert_eq!(*count, 2);
                assert_eq!(*sum, 11.0);
            }
            _ => panic!("wrong type"),
        }

        match &output["quux"].value() {
            MetricValue::AggregatedHistogram {
                buckets,
                count,
                sum,
            } => {
                // This index is _only_ stable so long as the offsets in
                // [`metrics::handle::Histogram::new`] are hard-coded. If this
                // check fails you might look there and see if we've allowed
                // users to set their own bucket widths.
                assert_eq!(buckets[9].count, 1);
                assert_eq!(buckets[10].count, 1);
                assert_eq!(*count, 2);
                assert_eq!(*sum, 16.1);
            }
            _ => panic!("wrong type"),
        }

        let labels = metric_tags!("host" => "foo");
        assert_eq!(Some(&labels), output["quux"].tags());
    }

    async fn event_from_config(config: InternalMetricsConfig) -> Event {
        let mut events = run_and_assert_source_compliance(
            config,
            time::Duration::from_millis(100),
            &SOURCE_TAGS,
        )
        .await;

        assert!(!events.is_empty());
        events.remove(0)
    }

    #[tokio::test]
    async fn default_namespace() {
        let event = event_from_config(InternalMetricsConfig::default()).await;

        assert_eq!(event.as_metric().namespace(), Some("vector"));
    }

    #[tokio::test]
    async fn sets_tags() {
        let event = event_from_config(InternalMetricsConfig {
            tags: TagsConfig {
                host_key: Some(OptionalValuePath::new("my_host_key")),
                pid_key: Some(String::from("my_pid_key")),
            },
            ..Default::default()
        })
        .await;

        let metric = event.as_metric();

        assert!(metric.tag_value("my_host_key").is_some());
        assert!(metric.tag_value("my_pid_key").is_some());
    }

    #[tokio::test]
    async fn only_host_tags_by_default() {
        let event = event_from_config(InternalMetricsConfig::default()).await;

        let metric = event.as_metric();

        assert!(metric.tag_value("host").is_some());
        assert!(metric.tag_value("pid").is_none());
    }

    #[tokio::test]
    async fn namespace() {
        let namespace = "totally_custom";

        let config = InternalMetricsConfig {
            namespace: namespace.to_owned(),
            ..InternalMetricsConfig::default()
        };

        let event = event_from_config(config).await;

        assert_eq!(event.as_metric().namespace(), Some(namespace));
    }

    #[test]
    fn test_topology_metrics_generation() {
        let mut topology = crate::config::TopologyMetadata::new();

        // Add a source -> transform connection
        topology.inputs.insert(
            ComponentKey::from("my_transform"),
            vec![OutputId {
                component: ComponentKey::from("my_source"),
                port: None,
            }],
        );

        // Add a transform -> sink connection
        topology.inputs.insert(
            ComponentKey::from("my_sink"),
            vec![OutputId {
                component: ComponentKey::from("my_transform"),
                port: Some("output1".to_string()),
            }],
        );

        // Add component types
        topology.component_types.insert(
            ComponentKey::from("my_source"),
            ("file".to_string(), "source".to_string()),
        );
        topology.component_types.insert(
            ComponentKey::from("my_transform"),
            ("remap".to_string(), "transform".to_string()),
        );
        topology.component_types.insert(
            ComponentKey::from("my_sink"),
            ("console".to_string(), "sink".to_string()),
        );

        let timestamp = Utc::now();
        let metrics = generate_topology_metrics(&topology, timestamp);

        // Should have 2 connection metrics
        assert_eq!(metrics.len(), 2);

        // Find the source -> transform connection
        let source_to_transform = metrics
            .iter()
            .find(|m| m.tags().and_then(|t| t.get("from_component_id")) == Some("my_source"))
            .expect("Should find source -> transform metric");

        assert_eq!(source_to_transform.name(), "component_connections");
        assert_eq!(source_to_transform.namespace(), Some("vector"));
        match source_to_transform.value() {
            MetricValue::Gauge { value } => assert_eq!(*value, 1.0),
            _ => panic!("Expected gauge metric"),
        }

        let tags1 = source_to_transform.tags().expect("Should have tags");
        assert_eq!(tags1.get("from_component_id"), Some("my_source"));
        assert_eq!(tags1.get("from_component_type"), Some("file"));
        assert_eq!(tags1.get("from_component_kind"), Some("source"));
        assert_eq!(tags1.get("to_component_id"), Some("my_transform"));
        assert_eq!(tags1.get("to_component_type"), Some("remap"));
        assert_eq!(tags1.get("to_component_kind"), Some("transform"));

        // Find the transform -> sink connection
        let transform_to_sink = metrics
            .iter()
            .find(|m| m.tags().and_then(|t| t.get("from_component_id")) == Some("my_transform"))
            .expect("Should find transform -> sink metric");

        let tags2 = transform_to_sink.tags().expect("Should have tags");
        assert_eq!(tags2.get("from_component_id"), Some("my_transform"));
        assert_eq!(tags2.get("from_output"), Some("output1"));
        assert_eq!(tags2.get("to_component_id"), Some("my_sink"));
    }
}
