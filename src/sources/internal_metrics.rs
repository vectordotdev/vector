use futures::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::time;
use tokio_stream::wrappers::IntervalStream;
use vector_core::ByteSizeOf;

use crate::{
    config::{DataType, Output, SourceConfig, SourceContext, SourceDescription},
    internal_events::{EventsReceived, StreamClosedError},
    metrics::Controller,
    shutdown::ShutdownSignal,
    SourceSender,
};

#[derive(Deserialize, Serialize, Debug, Clone, Derivative)]
#[derivative(Default)]
#[serde(deny_unknown_fields, default)]
pub struct InternalMetricsConfig {
    #[derivative(Default(value = "2.0"))]
    pub scrape_interval_secs: f64,
    pub tags: TagsConfig,
    pub namespace: Option<String>,
}

impl InternalMetricsConfig {
    /// Set the interval to collect internal metrics.
    pub fn scrape_interval_secs(&mut self, value: f64) {
        self.scrape_interval_secs = value;
    }
}

#[derive(Deserialize, Serialize, Debug, Clone, Derivative)]
#[derivative(Default)]
#[serde(deny_unknown_fields, default)]
pub struct TagsConfig {
    host_key: Option<String>,
    pid_key: Option<String>,
}

inventory::submit! {
    SourceDescription::new::<InternalMetricsConfig>("internal_metrics")
}

impl_generate_config_from_default!(InternalMetricsConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "internal_metrics")]
impl SourceConfig for InternalMetricsConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        if self.scrape_interval_secs == 0.0 {
            warn!(
                "Interval set to 0 secs, this could result in high CPU utilization. It is suggested to use interval >= 1 secs.",
            );
        }
        let interval = time::Duration::from_secs_f64(self.scrape_interval_secs);
        let namespace = self.namespace.clone();

        let host_key = self
            .tags
            .host_key
            .as_deref()
            .and_then(|tag| (!tag.is_empty()).then(|| tag.to_owned()));
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

    fn outputs(&self) -> Vec<Output> {
        vec![Output::default(DataType::Metric)]
    }

    fn source_type(&self) -> &'static str {
        "internal_metrics"
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}

struct InternalMetrics<'a> {
    namespace: Option<String>,
    host_key: Option<String>,
    pid_key: Option<String>,
    controller: &'a Controller,
    interval: time::Duration,
    out: SourceSender,
    shutdown: ShutdownSignal,
}

impl<'a> InternalMetrics<'a> {
    async fn run(mut self) -> Result<(), ()> {
        let mut interval =
            IntervalStream::new(time::interval(self.interval)).take_until(self.shutdown);
        while interval.next().await.is_some() {
            let hostname = crate::get_hostname();
            let pid = std::process::id().to_string();

            let metrics = self.controller.capture_metrics();
            let count = metrics.len();
            let byte_size = metrics.size_of();
            emit!(EventsReceived { count, byte_size });

            let batch = metrics.into_iter().map(|mut metric| {
                // A metric starts out with a default "vector" namespace, but will be overridden
                // if an explicit namespace is provided to this source.
                if let Some(namespace) = &self.namespace {
                    metric = metric.with_namespace(Some(namespace));
                }

                if let Some(host_key) = &self.host_key {
                    if let Ok(hostname) = &hostname {
                        metric.insert_tag(host_key.to_owned(), hostname.to_owned());
                    }
                }
                if let Some(pid_key) = &self.pid_key {
                    metric.insert_tag(pid_key.to_owned(), pid.clone());
                }
                metric
            });

            if let Err(error) = self.out.send_batch(batch).await {
                emit!(StreamClosedError { error, count });
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

    use super::*;
    use crate::{
        event::{
            metric::{Metric, MetricValue},
            Event,
        },
        metrics::Controller,
        SourceSender,
    };

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<InternalMetricsConfig>();
    }

    #[test]
    fn captures_internal_metrics() {
        let _ = crate::metrics::init_test();

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

        let mut labels = BTreeMap::new();
        labels.insert(String::from("host"), String::from("foo"));
        assert_eq!(Some(&labels), output["quux"].tags());
    }

    async fn event_from_config(config: InternalMetricsConfig) -> Event {
        let _ = crate::metrics::init_test();

        let (sender, mut recv) = SourceSender::new_test();

        tokio::spawn(async move {
            config
                .build(SourceContext::new_test(sender, None))
                .await
                .unwrap()
                .await
                .unwrap()
        });

        time::timeout(time::Duration::from_millis(100), recv.next())
            .await
            .expect("fetch metrics timeout")
            .expect("failed to get metrics from a stream")
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
                host_key: Some(String::from("my_host_key")),
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
    async fn no_tags_by_default() {
        let event = event_from_config(InternalMetricsConfig::default()).await;

        let metric = event.as_metric();

        assert!(metric.tag_value("my_host_key").is_none());
        assert!(metric.tag_value("my_pid_key").is_none());
    }

    #[tokio::test]
    async fn namespace() {
        let namespace = "totally_custom";

        let config = InternalMetricsConfig {
            namespace: Some(namespace.to_owned()),
            ..InternalMetricsConfig::default()
        };

        let event = event_from_config(config).await;

        assert_eq!(event.as_metric().namespace(), Some(namespace));
    }
}
