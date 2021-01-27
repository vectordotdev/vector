use crate::{
    config::{DataType, GlobalOptions, SourceConfig, SourceDescription},
    metrics::Controller,
    metrics::{capture_metrics, get_controller},
    shutdown::ShutdownSignal,
    Pipeline,
};
use futures::{stream, SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::time;

#[derive(Deserialize, Serialize, Debug, Clone, Derivative)]
#[derivative(Default)]
#[serde(deny_unknown_fields, default)]
pub struct InternalMetricsConfig {
    #[derivative(Default(value = "2"))]
    scrape_interval_secs: u64,
}

inventory::submit! {
    SourceDescription::new::<InternalMetricsConfig>("internal_metrics")
}

impl_generate_config_from_default!(InternalMetricsConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "internal_metrics")]
impl SourceConfig for InternalMetricsConfig {
    async fn build(
        &self,
        _name: &str,
        _globals: &GlobalOptions,
        shutdown: ShutdownSignal,
        out: Pipeline,
    ) -> crate::Result<super::Source> {
        if self.scrape_interval_secs == 0 {
            warn!(
                "Interval set to 0 secs, this could result in high CPU utilization. It is suggested to use interval >= 1 secs.",
            );
        }
        let interval = time::Duration::from_secs(self.scrape_interval_secs);
        Ok(Box::pin(run(get_controller()?, interval, out, shutdown)))
    }

    fn output_type(&self) -> DataType {
        DataType::Metric
    }

    fn source_type(&self) -> &'static str {
        "internal_metrics"
    }
}

async fn run(
    controller: &Controller,
    interval: time::Duration,
    out: Pipeline,
    shutdown: ShutdownSignal,
) -> Result<(), ()> {
    let mut out =
        out.sink_map_err(|error| error!(message = "Error sending internal metrics.", %error));

    let mut interval = time::interval(interval).take_until(shutdown);
    while interval.next().await.is_some() {
        let metrics = capture_metrics(controller);
        out.send_all(&mut stream::iter(metrics).map(Ok)).await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::event::metric::{Metric, MetricValue, StatisticKind};
    use crate::metrics::{capture_metrics, get_controller};
    use metrics::{counter, gauge, histogram};
    use std::collections::BTreeMap;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<super::InternalMetricsConfig>();
    }

    #[test]
    fn captures_internal_metrics() {
        let _ = crate::metrics::init();

        // There *seems* to be a race condition here (CI was flaky), so add a slight delay.
        std::thread::sleep(std::time::Duration::from_millis(300));

        gauge!("foo", 1.0);
        gauge!("foo", 2.0);
        counter!("bar", 3);
        counter!("bar", 4);
        histogram!("baz", 5.0);
        histogram!("baz", 6.0);
        histogram!("quux", 7.0, "host" => "foo");
        histogram!("quux", 8.0, "host" => "foo");

        let controller = get_controller().expect("no controller");

        // There *seems* to be a race condition here (CI was flaky), so add a slight delay.
        std::thread::sleep(std::time::Duration::from_millis(300));

        let output = capture_metrics(&controller)
            .map(|event| {
                let m = event.into_metric();
                (m.name().to_string(), m)
            })
            .collect::<BTreeMap<String, Metric>>();

        assert_eq!(MetricValue::Gauge { value: 2.0 }, output["foo"].data.value);
        assert_eq!(
            MetricValue::Counter { value: 7.0 },
            output["bar"].data.value
        );
        assert_eq!(
            MetricValue::Distribution {
                samples: crate::samples![5.0 => 1, 6.0 => 1],
                statistic: StatisticKind::Histogram
            },
            output["baz"].data.value
        );
        assert_eq!(
            MetricValue::Distribution {
                samples: crate::samples![7.0 => 1, 8.0 => 1],
                statistic: StatisticKind::Histogram
            },
            output["quux"].data.value
        );

        let mut labels = BTreeMap::new();
        labels.insert(String::from("host"), String::from("foo"));
        assert_eq!(Some(&labels), output["quux"].tags());
    }
}
