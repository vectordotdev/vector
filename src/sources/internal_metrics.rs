use crate::{
    config::{log_schema, DataType, SourceConfig, SourceContext, SourceDescription},
    metrics::Controller,
    shutdown::ShutdownSignal,
    Pipeline,
};
use futures::{stream, SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::time;
use tokio_stream::wrappers::IntervalStream;

#[derive(Deserialize, Serialize, Debug, Clone, Derivative)]
#[derivative(Default)]
#[serde(deny_unknown_fields, default)]
pub struct InternalMetricsConfig {
    #[derivative(Default(value = "2"))]
    scrape_interval_secs: u64,
    tags: TagsConfig,
    namespace: Option<String>,
    config_hash: Option<String>,
}

impl InternalMetricsConfig {
    /// Return an internal metrics config with enterprise reporting defaults.
    pub fn enterprise<T: Into<String>>(config_hash: T) -> Self {
        Self {
            namespace: Some("pipelines".to_owned()),
            config_hash: Some(config_hash.into()),
            ..Self::default()
        }
    }

    /// Set the interval to collect internal metrics.
    pub fn scrape_interval_secs(&mut self, value: u64) {
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
        if self.scrape_interval_secs == 0 {
            warn!(
                "Interval set to 0 secs, this could result in high CPU utilization. It is suggested to use interval >= 1 secs.",
            );
        }
        let interval = time::Duration::from_secs(self.scrape_interval_secs);
        let namespace = self.namespace.clone();
        let config_hash = self.config_hash.clone();
        let host_key = self.tags.host_key.as_deref().and_then(|tag| {
            if tag.is_empty() {
                None
            } else {
                Some(log_schema().host_key())
            }
        });
        let pid_key =
            self.tags
                .pid_key
                .as_deref()
                .and_then(|tag| if tag.is_empty() { None } else { Some("pid") });
        Ok(Box::pin(run(
            namespace,
            config_hash,
            host_key,
            pid_key,
            Controller::get()?,
            interval,
            cx.out,
            cx.shutdown,
        )))
    }

    fn output_type(&self) -> DataType {
        DataType::Metric
    }

    fn source_type(&self) -> &'static str {
        "internal_metrics"
    }
}

async fn run(
    namespace: Option<String>,
    config_hash: Option<String>,
    host_key: Option<&str>,
    pid_key: Option<&str>,
    controller: &Controller,
    interval: time::Duration,
    out: Pipeline,
    shutdown: ShutdownSignal,
) -> Result<(), ()> {
    let mut out =
        out.sink_map_err(|error| error!(message = "Error sending internal metrics.", %error));

    let mut interval = IntervalStream::new(time::interval(interval)).take_until(shutdown);
    while interval.next().await.is_some() {
        let hostname = crate::get_hostname();
        let pid = std::process::id().to_string();

        let metrics = controller.capture_metrics();

        out.send_all(&mut stream::iter(metrics).map(|mut metric| {
            // A metric starts out with a default "vector" namespace, but will be overridden
            // if an explicit namespace is provided to this source.
            if namespace.is_some() {
                metric = metric.with_namespace(namespace.as_ref());
            }

            // If a configuration hash is provided, report it. Used in enterprise.
            if let Some(config_hash) = &config_hash {
                metric.insert_tag("config_hash".to_owned(), config_hash.clone());
            }

            if let Some(host_key) = host_key {
                if let Ok(hostname) = &hostname {
                    metric.insert_tag(host_key.to_owned(), hostname.to_owned());
                }
            }
            if let Some(pid_key) = pid_key {
                metric.insert_tag(pid_key.to_owned(), pid.clone());
            }
            Ok(metric.into())
        }))
        .await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        event::{
            metric::{Metric, MetricValue},
            Event,
        },
        metrics::Controller,
        Pipeline,
    };
    use metrics::{counter, gauge, histogram};
    use std::collections::BTreeMap;

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
                assert_eq!(buckets[10].count, 2);
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

        let (sender, mut recv) = Pipeline::new_test();

        tokio::spawn(async move {
            config
                .build(SourceContext::new_test(sender))
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
