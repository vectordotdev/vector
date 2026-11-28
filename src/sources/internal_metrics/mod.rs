use std::time::Duration;
use vector_lib::internal_event::{emit, register};

use chrono::{DateTime, Utc};
use futures::StreamExt;
use serde_with::serde_as;
use tokio::time;
use tokio_stream::wrappers::IntervalStream;
use vector_lib::{
    ByteSizeOf, EstimatedJsonEncodedSizeOf,
    config::LogNamespace,
    configurable::configurable_component,
    internal_event::{
        ByteSize, BytesReceived, CountByteSize, GaugeName, InternalEventHandle as _, Protocol,
    },
    lookup::lookup_v2::OptionalValuePath,
};

use crate::{
    SourceSender,
    config::{
        SharedTopologyMetadata, SourceConfig, SourceContext, SourceOutput, TopologyMetadata,
        log_schema,
    },
    event::{Metric, MetricKind, MetricTags, MetricValue},
    internal_events::{EventsReceived, StreamClosedError},
    metrics::Controller,
    shutdown::ShutdownSignal,
};

mod delta;

use delta::DeltaState;

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

    pub tags: TagsConfig,

    /// Overrides the default namespace for the metrics emitted by the source.
    #[serde(default = "default_namespace")]
    pub namespace: String,

    /// Emit counters and histograms as incremental metrics holding the change since the previous
    /// scrape, instead of absolute totals since Vector started. Gauges stay absolute.
    ///
    /// Sinks that convert absolute metrics to incremental, such as `datadog_metrics`, drop the
    /// first value they see for each series, so counters and histograms observed within a single
    /// scrape interval are never sent. Enabling this option avoids that loss.
    ///
    /// This is off by default because it changes what other sinks receive:
    ///
    /// - Counts start from when the source is built, not from when Vector started.
    /// - Sinks that write metrics as-is, such as `splunk_hec_metrics` or codec-based sinks, report
    ///   per-scrape deltas instead of running totals. So do `influxdb_metrics` and
    ///   `greptimedb_metrics` for histograms.
    /// - The `opentelemetry` sink and the `otlp` codec switch to delta temporality.
    /// - Sinks that require an interval on incremental counters, such as `new_relic`, drop them.
    ///
    /// When `expire_metrics_secs` is set, using several `internal_metrics` sources with this option
    /// enabled is not recommended: a series expired and recreated between two scrapes can be
    /// under-counted.
    #[serde(default)]
    pub emit_incremental: bool,
}

impl Default for InternalMetricsConfig {
    fn default() -> Self {
        Self {
            scrape_interval_secs: default_scrape_interval(),
            tags: TagsConfig::default(),
            namespace: default_namespace(),
            emit_incremental: false,
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

        let controller = Controller::get()?;

        // Scrape now so that a source built after Vector has already been running does not report
        // the whole accumulated registry as one enormous first increment. Also covers reloads,
        // which rebuild sources.
        let deltas = self.emit_incremental.then(|| {
            let mut deltas = DeltaState::default();
            deltas.seed(controller.capture_metrics());
            deltas
        });

        Ok(crate::sources::source_with_error(
            InternalMetrics {
                namespace,
                host_key,
                pid_key,
                controller,
                deltas,
                interval,
                out: cx.out,
                shutdown: cx.shutdown,
                topology_metadata: cx.topology_metadata,
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
    deltas: Option<DeltaState>,
    interval: time::Duration,
    out: SourceSender,
    shutdown: ShutdownSignal,
    topology_metadata: Option<SharedTopologyMetadata>,
}

impl InternalMetrics<'_> {
    async fn run(mut self) -> vector_lib::source_sender::SendResult {
        let events_received = register(EventsReceived);
        let bytes_received = register(BytesReceived::from(Protocol::INTERNAL));
        let mut interval =
            IntervalStream::new(time::interval(self.interval)).take_until(self.shutdown);
        while interval.next().await.is_some() {
            let hostname = crate::get_hostname();
            let pid = std::process::id().to_string();

            let mut metrics = self.controller.capture_metrics();
            if let Some(deltas) = &mut self.deltas {
                deltas.convert(&mut metrics);
            }

            // The topology connection gauges are not stored in the metrics registry: they are
            // derived from the running topology on every scrape, so they always reflect the
            // currently loaded configuration and disappear as soon as a connection is removed.
            if let Some(topology_metadata) = &self.topology_metadata {
                let timestamp = metrics
                    .first()
                    .and_then(Metric::timestamp)
                    .unwrap_or_else(Utc::now);
                let topology = topology_metadata
                    .read()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                metrics.extend(topology_connection_metrics(&topology, timestamp));
            }

            let count = metrics.len();
            let byte_size = metrics.size_of();
            let json_size = metrics.estimated_json_encoded_size_of();

            bytes_received.emit(ByteSize(byte_size));
            events_received.emit(CountByteSize(count, json_size));

            let batch = metrics.into_iter().map(|mut metric| {
                // A metric starts out with a default "vector" namespace, but will be overridden
                // if an explicit namespace is provided to this source.
                if self.namespace != "vector" {
                    metric = metric.with_namespace(Some(self.namespace.clone()));
                }

                if let Some(host_key) = &self.host_key.path
                    && let Ok(hostname) = &hostname
                {
                    metric.replace_tag(host_key.to_string(), hostname.to_owned());
                }
                if let Some(pid_key) = &self.pid_key {
                    metric.replace_tag(pid_key.to_owned(), pid.clone());
                }
                metric
            });

            if let Err(error) = self.out.send_batch(batch).await {
                emit(StreamClosedError { count });
                return Err(error);
            }
        }

        Ok(())
    }
}

/// Builds one `component_connections` gauge per edge in the topology.
///
/// Each gauge has a value of `1` and is tagged with the IDs, types, and kinds of the components on
/// either end of the edge, plus the named output of the upstream component, if any.
fn topology_connection_metrics(
    topology: &TopologyMetadata,
    timestamp: DateTime<Utc>,
) -> impl Iterator<Item = Metric> + '_ {
    topology
        .inputs
        .iter()
        .flat_map(move |(to_component, inputs)| {
            inputs.iter().map(move |input| {
                let mut tags = MetricTags::default();

                tags.replace("from_component_id".to_owned(), input.component.to_string());
                if let Some((type_name, kind)) = topology.component_types.get(&input.component) {
                    tags.replace("from_component_type".to_owned(), (*type_name).to_owned());
                    tags.replace("from_component_kind".to_owned(), (*kind).to_owned());
                }
                if let Some(port) = &input.port {
                    tags.replace("from_output".to_owned(), port.clone());
                }

                tags.replace("to_component_id".to_owned(), to_component.to_string());
                if let Some((type_name, kind)) = topology.component_types.get(to_component) {
                    tags.replace("to_component_type".to_owned(), (*type_name).to_owned());
                    tags.replace("to_component_kind".to_owned(), (*kind).to_owned());
                }

                Metric::new(
                    GaugeName::ComponentConnections.as_str(),
                    MetricKind::Absolute,
                    MetricValue::Gauge { value: 1.0 },
                )
                .with_namespace(Some("vector"))
                .with_tags(Some(tags))
                .with_timestamp(Some(timestamp))
            })
        })
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        sync::{Arc, RwLock},
    };

    use strum::IntoEnumIterator;
    use vector_lib::{
        counter, gauge, histogram,
        internal_event::{CounterName, GaugeName, HistogramName},
        metric_tags,
        metrics::Controller,
    };

    use super::*;
    use crate::{
        config::{ComponentKey, OutputId},
        event::{
            Event,
            metric::{Metric, MetricKind, MetricValue},
        },
        test_util::{
            self, collect_ready,
            components::{SOURCE_TAGS, run_and_assert_source_compliance},
        },
    };

    fn incremental_config() -> InternalMetricsConfig {
        InternalMetricsConfig {
            emit_incremental: true,
            ..Default::default()
        }
    }

    /// Builds and runs the source for a single scrape, letting the caller record metrics in
    /// between so they land after the build-time seed. The default 1s interval fires once
    /// immediately, so exactly one scrape is collected.
    async fn scrape_once(config: InternalMetricsConfig, record: impl FnOnce()) -> Vec<Metric> {
        test_util::trace_init();

        let (tx, rx) = SourceSender::new_test();
        let source = config
            .build(SourceContext::new_test(tx, None))
            .await
            .unwrap();

        record();

        tokio::spawn(source);
        time::sleep(time::Duration::from_millis(100)).await;

        collect_ready(rx)
            .into_iter()
            .map(Event::into_metric)
            .collect()
    }

    fn find<'a>(metrics: &'a [Metric], name: &str) -> &'a Metric {
        metrics
            .iter()
            .find(|metric| metric.name() == name)
            .unwrap_or_else(|| panic!("{name} not emitted, got {metrics:?}"))
    }

    #[test]
    fn generate_config() {
        test_util::test_generate_config::<InternalMetricsConfig>();
    }

    #[test]
    fn captures_internal_metrics() {
        test_util::trace_init();

        // There *seems* to be a race condition here (CI was flaky), so add a slight delay.
        std::thread::sleep(std::time::Duration::from_millis(300));

        let gauge_name = GaugeName::iter().next().unwrap();
        let counter_name = CounterName::iter().next().unwrap();
        let mut histogram_iter = HistogramName::iter();
        let histogram_name = histogram_iter.next().unwrap();
        let histogram_tagged_name = histogram_iter.next().unwrap();

        gauge!(gauge_name).set(1.0);
        gauge!(gauge_name).set(2.0);
        counter!(counter_name).increment(3);
        counter!(counter_name).increment(4);
        histogram!(histogram_name).record(5.0);
        histogram!(histogram_name).record(6.0);
        histogram!(histogram_tagged_name, "host" => "foo").record(8.0);
        histogram!(histogram_tagged_name, "host" => "foo").record(8.1);

        let controller = Controller::get().expect("no controller");

        // There *seems* to be a race condition here (CI was flaky), so add a slight delay.
        std::thread::sleep(std::time::Duration::from_millis(300));

        let output = controller
            .capture_metrics()
            .into_iter()
            .map(|metric| (metric.name().to_string(), metric))
            .collect::<BTreeMap<String, Metric>>();

        assert_eq!(
            &MetricValue::Gauge { value: 2.0 },
            output[gauge_name.as_str()].value()
        );
        assert_eq!(
            &MetricValue::Counter { value: 7.0 },
            output[counter_name.as_str()].value()
        );

        match &output[histogram_name.as_str()].value() {
            MetricValue::AggregatedHistogram {
                buckets,
                count,
                sum,
            } => {
                // This index is _only_ stable so long as the offsets in
                // [`metrics::handle::Histogram::new`] are hard-coded. If this
                // check fails you might look there and see if we've allowed
                // users to set their own bucket widths.
                assert_eq!(buckets[15].count, 2);
                assert_eq!(*count, 2);
                assert_eq!(*sum, Some(11.0));
            }
            _ => panic!("wrong type"),
        }

        match &output[histogram_tagged_name.as_str()].value() {
            MetricValue::AggregatedHistogram {
                buckets,
                count,
                sum,
            } => {
                // This index is _only_ stable so long as the offsets in
                // [`metrics::handle::Histogram::new`] are hard-coded. If this
                // check fails you might look there and see if we've allowed
                // users to set their own bucket widths.
                assert_eq!(buckets[15].count, 1);
                assert_eq!(buckets[16].count, 1);
                assert_eq!(*count, 2);
                assert_eq!(*sum, Some(16.1));
            }
            _ => panic!("wrong type"),
        }

        let labels = metric_tags!("host" => "foo");
        assert_eq!(Some(&labels), output[histogram_tagged_name.as_str()].tags());
    }

    async fn event_from_config(config: InternalMetricsConfig) -> Event {
        let mut events = run_and_assert_source_compliance(
            config,
            time::Duration::from_millis(100),
            &SOURCE_TAGS,
        )
        .await;

        assert_ne!(events, [] as [vector_lib::event::Event; 0]);
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

    /// Every observation lands inside a single scrape interval. As absolute metrics, a sink
    /// converting them to increments would only record a baseline and emit nothing.
    #[tokio::test]
    async fn emits_increments_for_counters_and_histograms() {
        let counter_name = CounterName::iter().next().unwrap();
        let histogram_name = HistogramName::iter().next().unwrap();

        let metrics = scrape_once(incremental_config(), || {
            counter!(counter_name).increment(3);
            histogram!(histogram_name).record(5.0);
            histogram!(histogram_name).record(6.0);
        })
        .await;

        let counter = find(&metrics, counter_name.as_str());
        assert_eq!(counter.kind(), MetricKind::Incremental);
        assert_eq!(counter.value(), &MetricValue::Counter { value: 3.0 });

        let histogram = find(&metrics, histogram_name.as_str());
        assert_eq!(histogram.kind(), MetricKind::Incremental);
        match histogram.value() {
            MetricValue::AggregatedHistogram { count, sum, .. } => {
                assert_eq!(*count, 2);
                assert_eq!(*sum, Some(11.0));
            }
            value => panic!("wrong type: {value:?}"),
        }
    }

    #[tokio::test]
    async fn absolute_by_default() {
        let counter_name = CounterName::iter().next().unwrap();

        let metrics = scrape_once(InternalMetricsConfig::default(), || {
            counter!(counter_name).increment(3);
        })
        .await;

        let counter = find(&metrics, counter_name.as_str());
        assert_eq!(counter.kind(), MetricKind::Absolute);
        assert_eq!(counter.value(), &MetricValue::Counter { value: 3.0 });
    }

    #[tokio::test]
    async fn gauges_stay_absolute() {
        let gauge_name = GaugeName::iter().next().unwrap();

        let metrics = scrape_once(incremental_config(), || gauge!(gauge_name).set(2.0)).await;

        let gauge = find(&metrics, gauge_name.as_str());
        assert_eq!(gauge.kind(), MetricKind::Absolute);
        assert_eq!(gauge.value(), &MetricValue::Gauge { value: 2.0 });

        let cardinality = find(&metrics, "internal_metrics_cardinality");
        assert_eq!(cardinality.kind(), MetricKind::Absolute);
    }

    /// The build-time scrape keeps a source added to an already-running Vector from reporting the
    /// whole accumulated registry as its first increment.
    #[tokio::test]
    async fn build_time_scrape_excludes_prior_activity() {
        let counter_name = CounterName::iter().next().unwrap();

        // Not `scrape_once`: its `trace_init` clears the registry, which would erase the prior
        // activity this test relies on.
        test_util::trace_init();
        counter!(counter_name).increment(10);

        let (tx, rx) = SourceSender::new_test();
        let source = incremental_config()
            .build(SourceContext::new_test(tx, None))
            .await
            .unwrap();

        counter!(counter_name).increment(5);

        tokio::spawn(source);
        time::sleep(time::Duration::from_millis(100)).await;

        let metrics = collect_ready(rx)
            .into_iter()
            .map(Event::into_metric)
            .collect::<Vec<_>>();

        // 5, not the 15 now held by the registry.
        assert_eq!(
            find(&metrics, counter_name.as_str()).value(),
            &MetricValue::Counter { value: 5.0 }
        );
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

    fn test_topology() -> TopologyMetadata {
        let mut topology = TopologyMetadata::default();
        topology.inputs.insert(
            ComponentKey::from("my_transform"),
            vec![OutputId::from(ComponentKey::from("my_source"))],
        );
        topology.inputs.insert(
            ComponentKey::from("my_sink"),
            vec![OutputId {
                component: ComponentKey::from("my_transform"),
                port: Some("output1".to_owned()),
            }],
        );
        topology
            .component_types
            .insert(ComponentKey::from("my_source"), ("file", "source"));
        topology
            .component_types
            .insert(ComponentKey::from("my_transform"), ("route", "transform"));
        topology
            .component_types
            .insert(ComponentKey::from("my_sink"), ("console", "sink"));
        topology
    }

    fn connection_from<'a>(metrics: &'a [Metric], from_component_id: &str) -> &'a Metric {
        metrics
            .iter()
            .find(|metric| {
                metric.name() == "component_connections"
                    && metric.tag_value("from_component_id").as_deref() == Some(from_component_id)
            })
            .unwrap_or_else(|| panic!("no connection from {from_component_id} in {metrics:?}"))
    }

    #[test]
    fn topology_connection_metrics_tags_each_edge() {
        let topology = test_topology();
        let timestamp = Utc::now();
        let metrics = topology_connection_metrics(&topology, timestamp).collect::<Vec<_>>();

        assert_eq!(metrics.len(), 2);

        let source_to_transform = connection_from(&metrics, "my_source");
        assert_eq!(source_to_transform.namespace(), Some("vector"));
        assert_eq!(source_to_transform.kind(), MetricKind::Absolute);
        assert_eq!(
            source_to_transform.value(),
            &MetricValue::Gauge { value: 1.0 }
        );
        assert_eq!(source_to_transform.timestamp(), Some(timestamp));
        let tags = source_to_transform.tags().expect("tags");
        assert_eq!(tags.get("from_component_type"), Some("file"));
        assert_eq!(tags.get("from_component_kind"), Some("source"));
        assert_eq!(tags.get("from_output"), None);
        assert_eq!(tags.get("to_component_id"), Some("my_transform"));
        assert_eq!(tags.get("to_component_type"), Some("route"));
        assert_eq!(tags.get("to_component_kind"), Some("transform"));

        let transform_to_sink = connection_from(&metrics, "my_transform");
        let tags = transform_to_sink.tags().expect("tags");
        assert_eq!(tags.get("from_output"), Some("output1"));
        assert_eq!(tags.get("to_component_id"), Some("my_sink"));
        assert_eq!(tags.get("to_component_type"), Some("console"));
        assert_eq!(tags.get("to_component_kind"), Some("sink"));
    }

    #[tokio::test]
    async fn emits_topology_connections_with_common_tags() {
        test_util::trace_init();

        let config = InternalMetricsConfig {
            namespace: "custom".to_owned(),
            tags: TagsConfig {
                host_key: Some(OptionalValuePath::new("my_host")),
                pid_key: Some("my_pid".to_owned()),
            },
            ..Default::default()
        };

        let (tx, rx) = SourceSender::new_test();
        let mut cx = SourceContext::new_test(tx, None);
        cx.topology_metadata = Some(Arc::new(RwLock::new(test_topology())));
        let source = config.build(cx).await.unwrap();

        tokio::spawn(source);
        time::sleep(time::Duration::from_millis(100)).await;

        let metrics = collect_ready(rx)
            .into_iter()
            .map(Event::into_metric)
            .collect::<Vec<_>>();

        let connection = connection_from(&metrics, "my_transform");
        assert_eq!(connection.namespace(), Some("custom"));
        assert_eq!(
            connection.tag_value("my_pid"),
            Some(std::process::id().to_string())
        );
        assert_eq!(connection.tag_value("my_host"), crate::get_hostname().ok());
        assert_eq!(
            metrics
                .iter()
                .filter(|metric| metric.name() == "component_connections")
                .count(),
            2
        );
    }

    #[tokio::test]
    async fn no_topology_connections_without_topology() {
        let metrics = scrape_once(InternalMetricsConfig::default(), || {}).await;
        assert!(
            metrics
                .iter()
                .all(|metric| metric.name() != "component_connections")
        );
    }
}
