use crate::{
    event::metric::{Metric, MetricKind, MetricValue},
    shutdown::ShutdownSignal,
    topology::config::{DataType, GlobalOptions, SourceConfig, SourceDescription},
    Event,
};
use chrono::Utc;
use futures::{
    compat::Future01CompatExt,
    future::{FutureExt, TryFutureExt},
    stream::StreamExt,
};
use futures01::{sync::mpsc, Future, Sink};
use metrics_core::Key;
use metrics_runtime::{Controller, Measurement};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, time::Duration};
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
        out: mpsc::Sender<Event>,
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

fn get_controller() -> crate::Result<Controller> {
    crate::metrics::CONTROLLER
        .get()
        .cloned()
        .ok_or("metrics system not initialized".into())
}

async fn run(
    controller: Controller,
    mut out: mpsc::Sender<Event>,
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
            .expect("sending??");
        out = sink;
    }

    Ok(())
}

fn capture_metrics(controller: &Controller) -> impl Iterator<Item = Event> {
    controller
        .snapshot()
        .into_measurements()
        .into_iter()
        .map(|(k, m)| into_event(k, m))
}

fn into_event(key: Key, measurement: Measurement) -> Event {
    let value = match measurement {
        Measurement::Counter(v) => MetricValue::Counter { value: v as f64 },
        Measurement::Gauge(v) => MetricValue::Gauge { value: v as f64 },
        Measurement::Histogram(packed) => {
            let values = packed
                .decompress()
                .into_iter()
                .map(|i| i as f64)
                .collect::<Vec<_>>();
            let sample_rates = vec![1; values.len()];
            MetricValue::Distribution {
                values,
                sample_rates,
            }
        }
    };

    let labels = key
        .labels()
        .map(|label| (String::from(label.key()), String::from(label.value())))
        .collect::<BTreeMap<_, _>>();

    let metric = Metric {
        name: key.name().to_string(),
        timestamp: Some(Utc::now()),
        tags: if labels.len() == 0 {
            None
        } else {
            Some(labels)
        },
        kind: MetricKind::Absolute,
        value,
    };

    Event::Metric(metric)
}

#[cfg(test)]
mod tests {
    use super::{capture_metrics, get_controller};
    use crate::event::metric::{Metric, MetricValue};
    use metrics::{counter, gauge, timing, value};
    use std::collections::BTreeMap;

    #[test]
    fn captures_internal_metrics() {
        crate::metrics::init().unwrap();

        let controller = get_controller().expect("no controller");

        gauge!("foo", 1);
        gauge!("foo", 2);
        counter!("bar", 3);
        counter!("bar", 4);
        timing!("baz", 5);
        timing!("baz", 6);
        value!("quux", 7, "host" => "foo");
        value!("quux", 8, "host" => "foo");

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
                sample_rates: vec![1, 1]
            },
            output["baz"].value
        );
        assert_eq!(
            MetricValue::Distribution {
                values: vec![7.0, 8.0],
                sample_rates: vec![1, 1]
            },
            output["quux"].value
        );

        let mut labels = BTreeMap::new();
        labels.insert(String::from("host"), String::from("foo"));
        assert_eq!(Some(labels), output["quux"].tags);
    }
}
