use crate::{
    config::{DataType, GlobalOptions, SourceConfig, SourceDescription},
    metrics::{capture_metrics, get_controller},
    shutdown::ShutdownSignal,
    Pipeline,
};
use futures::{
    compat::Future01CompatExt,
    future::{FutureExt, TryFutureExt},
    stream::StreamExt,
};
use futures01::{Future, Sink};
use metrics_runtime::Controller;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::time::interval;

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
pub struct InternalMetricsConfig;

inventory::submit! {
    SourceDescription::new::<InternalMetricsConfig>("internal_metrics")
}

#[typetag::serde(name = "internal_metrics")]
impl SourceConfig for InternalMetricsConfig {
    fn build(
        &self,
        _name: &str,
        _globals: &GlobalOptions,
        shutdown: ShutdownSignal,
        out: Pipeline,
    ) -> crate::Result<super::Source> {
        let fut = run(get_controller()?, out, shutdown).boxed().compat();
        Ok(Box::new(fut))
    }

    fn output_type(&self) -> DataType {
        DataType::Metric
    }

    fn source_type(&self) -> &'static str {
        "internal_metrics"
    }
}

async fn run(
    controller: Controller,
    mut out: Pipeline,
    mut shutdown: ShutdownSignal,
) -> Result<(), ()> {
    let mut interval = interval(Duration::from_secs(2)).map(|_| ());

    while let Some(()) = interval.next().await {
        // Check for shutdown signal
        if shutdown.poll().expect("polling shutdown").is_ready() {
            break;
        }

        let metrics = capture_metrics(&controller);

        let (sink, _) = out
            .send_all(futures01::stream::iter_ok(metrics))
            .compat()
            .await
            .map_err(|error| error!(message = "Error sending internal metrics", %error))?;
        out = sink;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::event::metric::{Metric, MetricValue, StatisticKind};
    use crate::metrics::{capture_metrics, get_controller};
    use metrics::{counter, gauge, timing, value};
    use std::collections::BTreeMap;

    #[test]
    fn captures_internal_metrics() {
        let _ = crate::metrics::init();

        // There *seems* to be a race condition here (CI was flaky), so add a slight delay.
        std::thread::sleep(std::time::Duration::from_millis(300));

        gauge!("foo", 1);
        gauge!("foo", 2);
        counter!("bar", 3);
        counter!("bar", 4);
        timing!("baz", 5);
        timing!("baz", 6);
        value!("quux", 7, "host" => "foo");
        value!("quux", 8, "host" => "foo");

        let controller = get_controller().expect("no controller");

        // There *seems* to be a race condition here (CI was flaky), so add a slight delay.
        std::thread::sleep(std::time::Duration::from_millis(300));

        let output = capture_metrics(&controller)
            .map(|event| {
                let m = event.into_metric();
                (m.name.clone(), m)
            })
            .collect::<BTreeMap<String, Metric>>();

        assert_eq!(MetricValue::Gauge { value: 2.0 }, output["foo"].value);
        assert_eq!(MetricValue::Counter { value: 7.0 }, output["bar"].value);
        assert_eq!(
            MetricValue::Distribution {
                values: vec![5.0, 6.0],
                sample_rates: vec![1, 1],
                statistic: StatisticKind::Histogram
            },
            output["baz"].value
        );
        assert_eq!(
            MetricValue::Distribution {
                values: vec![7.0, 8.0],
                sample_rates: vec![1, 1],
                statistic: StatisticKind::Histogram
            },
            output["quux"].value
        );

        let mut labels = BTreeMap::new();
        labels.insert(String::from("host"), String::from("foo"));
        assert_eq!(Some(labels), output["quux"].tags);
    }
}
