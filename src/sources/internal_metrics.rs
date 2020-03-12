use crate::{
    shutdown::ShutdownSignal,
    event::metric::{Metric, MetricKind, MetricValue},
    topology::config::{DataType, GlobalOptions, SourceConfig, SourceDescription},
    Event,
};
use chrono::Utc;
use futures::{
    compat::{Future01CompatExt, Stream01CompatExt},
    future::{FutureExt, TryFutureExt},
    stream::StreamExt,
};
use futures01::{sync::mpsc, Future, Sink};
use metrics_core::Key;
use metrics_runtime::{Controller, Measurement};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, time::Duration};
use stream_cancel::Tripwire;
use tokio01::timer::Interval;

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
        _shutdown: ShutdownSignal,
        out: mpsc::Sender<Event>,
    ) -> crate::Result<super::Source> {
        let (trigger, tripwire) = Tripwire::new();
        trigger.disable(); // TODO: don't actually run forever
        let fut = run(get_controller()?, out, tripwire).boxed().compat();
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
    mut tripwire: Tripwire,
) -> Result<(), ()> {
    let mut interval = Interval::new_interval(Duration::from_secs(2))
        .compat()
        .map(|_| ());

    while let Some(()) = interval.next().await {
        // Check for shutdown signal
        if tripwire.poll().expect("polling tripwire").is_ready() {
            break;
        }

        let metrics = controller
            .snapshot()
            .into_measurements()
            .into_iter()
            .map(|(k, m)| into_event(k, m));

        let (sink, _) = out
            .send_all(futures01::stream::iter_ok(metrics))
            .compat()
            .await
            .expect("sending??");
        out = sink;
    }

    Ok(())
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
    use super::{get_controller, run};
    use crate::{
        event::metric::{Metric, MetricValue},
        test_util::{collect_n, runtime},
    };
    use futures01::sync::mpsc;
    use metrics::{counter, gauge, timing, value};
    use std::{collections::BTreeMap, thread, time::Duration};
    use stream_cancel::Tripwire;

    #[test]
    fn captures_internal_metrics() {
        crate::metrics::init();
        let mut runtime = runtime();

        let controller = get_controller().expect("no controller");
        let (tx, rx) = mpsc::channel(10);
        let (trigger, tripwire) = Tripwire::new();
        runtime.executor().spawn_std(async {
            run(controller, tx, tripwire).await.unwrap();
        });

        gauge!("foo", 1);
        gauge!("foo", 2);
        counter!("bar", 3);
        counter!("bar", 4);
        timing!("baz", 5);
        timing!("baz", 6);
        value!("quux", 7, "host" => "foo");
        value!("quux", 8, "host" => "foo");

        // TODO: split out function from `run` so we can drive it without sleeping
        thread::sleep(Duration::from_secs(5));
        drop(trigger);

        let output = runtime
            .block_on(collect_n(rx, 4))
            .unwrap()
            .into_iter()
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
