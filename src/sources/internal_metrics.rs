use std::time::Duration;

use futures::StreamExt;
use serde_with::serde_as;
use tokio::time;
use tokio_stream::wrappers::IntervalStream;
use vector_lib::configurable::configurable_component;
use vector_lib::internal_event::{CountByteSize, InternalEventHandle as _};
use vector_lib::lookup::lookup_v2::OptionalValuePath;
use vector_lib::{config::LogNamespace, ByteSizeOf, EstimatedJsonEncodedSizeOf};

use crate::{
    config::{log_schema, SourceConfig, SourceContext, SourceOutput},
    internal_events::{EventsReceived, InternalMetricsBytesReceived, StreamClosedError},
    metrics::Controller,
    shutdown::ShutdownSignal,
    SourceSender,
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
#[derive(Clone, Debug)]
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
    #[serde(default = "default_host_key")]
    pub host_key: OptionalValuePath,

    /// Sets the name of the tag to use to add the current process ID to each metric.
    ///
    ///
    /// By default, this is not set and the tag is not automatically added.
    #[configurable(metadata(docs::examples = "pid"))]
    pub pid_key: Option<String>,
}

impl Default for TagsConfig {
    fn default() -> Self {
        Self {
            host_key: default_host_key(),
            pid_key: None,
        }
    }
}

fn default_scrape_interval() -> Duration {
    Duration::from_secs_f64(1.0)
}

fn default_namespace() -> String {
    "vector".to_owned()
}

fn default_host_key() -> OptionalValuePath {
    log_schema().host_key().cloned().into()
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

        let host_key = self.tags.host_key.clone();

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
}

impl<'a> InternalMetrics<'a> {
    async fn run(mut self) -> Result<(), ()> {
        let events_received = register!(EventsReceived);
        let mut interval =
            IntervalStream::new(time::interval(self.interval)).take_until(self.shutdown);
        while interval.next().await.is_some() {
            let hostname = crate::get_hostname();
            let pid = std::process::id().to_string();

            let metrics = self.controller.capture_metrics();
            let count = metrics.len();
            let byte_size = metrics.size_of();
            let json_size = metrics.estimated_json_encoded_size_of();

            emit!(InternalMetricsBytesReceived { byte_size });
            events_received.emit(CountByteSize(count, json_size));

            let batch = metrics.into_iter().map(|mut metric| {
                // A metric starts out with a default "vector" namespace, but will be overridden
                // if an explicit namespace is provided to this source.
                if self.namespace != "vector" {
                    metric = metric.with_namespace(Some(self.namespace.clone()));
                }

                if let Some(host_key) = &self.host_key.path {
                    if let Ok(hostname) = &hostname {
                        metric.replace_tag(host_key.to_string(), hostname.to_owned());
                    }
                }
                if let Some(pid_key) = &self.pid_key {
                    metric.replace_tag(pid_key.to_owned(), pid.clone());
                }
                metric
            });

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
    use std::collections::BTreeMap;

    use metrics::{counter, gauge, histogram};
    use vector_lib::metric_tags;

    use super::*;
    use crate::{
        event::{
            metric::{Metric, MetricValue},
            Event,
        },
        metrics::Controller,
        test_util::{
            self,
            components::{run_and_assert_source_compliance, SOURCE_TAGS},
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

        gauge!("foo", 1.0);
        gauge!("foo", 2.0);
        counter!("bar", 3);
        counter!("bar", 4);
        histogram!("baz", 5.0);
        histogram!("baz", 6.0);
        histogram!("quux", 8.0, "host" => "foo");
        histogram!("quux", 8.1, "host" => "foo");

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
                host_key: OptionalValuePath::new("my_host_key"),
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
}
