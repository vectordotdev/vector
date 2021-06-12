// let g:cargo_makeprg_params = = '--lib --no-default-features --features=transforms-aggregate transforms::aggregate'
use crate::{
    internal_events::{AggregateEventRecorded, AggregateFlushed},
    transforms::{
        TaskTransform,
        Transform,
    },
    config::{DataType, GlobalOptions, TransformConfig, TransformDescription},
    event::{
        metric,
        Event,
        EventMetadata,
    },
};
use async_stream::stream;
use futures::{stream, Stream, StreamExt};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    pin::Pin,
    time::{Duration},
};

#[derive(Deserialize, Serialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct AggregateConfig {
    pub interval_ms: Option<u64>,
}

inventory::submit! {
    TransformDescription::new::<AggregateConfig>("aggregate")
}

impl_generate_config_from_default!(AggregateConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "aggregate")]
impl TransformConfig for AggregateConfig {
    async fn build(&self, _globals: &GlobalOptions) -> crate::Result<Transform> {
        Aggregate::new(self).map(Transform::task)
    }

    fn input_type(&self) -> DataType {
        DataType::Metric
    }

    fn output_type(&self) -> DataType {
        DataType::Metric
    }

    fn transform_type(&self) -> &'static str {
        "aggregate"
    }
}


#[derive(Debug)]
pub struct Aggregate {
    interval: Duration,
    map_a: HashMap<metric::MetricSeries, metric::MetricData>,
    map_b: HashMap<metric::MetricSeries, metric::MetricData>,
    using_b: bool,
}

impl Aggregate {
    pub fn new(config: &AggregateConfig) -> crate::Result<Self> {
        Ok(Self {
            interval: Duration::from_millis(config.interval_ms.unwrap_or(10 * 1000)),
            map_a: HashMap::new(),
            map_b: HashMap::new(),
            using_b: false,
        })
    }

    fn record(&mut self, event: Event) {
        let metric = event.as_metric();
        let series = metric.series();
        let data = metric.data();

        let map = match self.using_b {
            true => &mut self.map_b,
            false => &mut self.map_a,
        };

        match data.kind {
            metric::MetricKind::Incremental => {
                match map.get_mut(&series) {
                    // We already have something, add to it
                    Some(existing) => existing.update(data),
                    None => {
                        // New so store
                        map.insert(series.clone(), data.clone());
                        true
                    }
                };
            },
            metric::MetricKind::Absolute => {
                // Always store
                map.insert(series.clone(), data.clone());
            }
        };

        emit!(AggregateEventRecorded);
    }

    fn flush_into(&mut self, output: &mut Vec<Event>) -> u64 {
        let mut count = 0_u64;

        // TODO: locking/safety etc... we can also only have 1 call into flush at a time
        let map = match self.using_b {
            true => {
                self.using_b = false;
                &mut self.map_b
            },
            false => {
                self.using_b = true;
                &mut self.map_a
            },
        };

        if map.len() > 0 {
            // TODO: not clear how this should work with aggregation so just stuffing a default one
            // in for now.
            let metadata = EventMetadata::default();

            for (series, metric) in map.drain() {
                let metric = metric::Metric::from_parts(series, metric, metadata.clone());
                output.push(Event::Metric(metric));
                count += 1;
            }
        }

        emit!(AggregateFlushed);
        return count;
    }
}

impl TaskTransform for Aggregate {
    fn transform(
        self: Box<Self>,
        mut input_rx: Pin<Box<dyn Stream<Item = Event> + Send>>,
    ) -> Pin<Box<dyn Stream<Item = Event> + Send>>
    where
        Self: 'static,
    {
        let mut me = self;

        let interval = me.interval;

        let mut flush_stream = tokio::time::interval(interval);

        Box::pin(
            stream! {
                loop {
                    let mut output = Vec::new();
                    let done = tokio::select! {
                        _ = flush_stream.tick() => {
                            me.flush_into(&mut output);
                            false
                        }
                        maybe_event = input_rx.next() => {
                            match maybe_event {
                                None => {
                                    me.flush_into(&mut output);
                                    true
                                }
                                Some(event) => {
                                    me.record(event);
                                    false
                                }
                            }
                        }
                    };
                    yield stream::iter(output.into_iter());
                    if done { break }
                }
            }
            .flatten(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use crate::{event::metric, event::Event, event::Metric};

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<AggregateConfig>();
    }

    fn make_metric(
        name: &'static str,
        kind: metric::MetricKind,
        value: metric::MetricValue,
        tags: BTreeMap<String, String>,
    ) -> Event {
        Event::Metric(
            Metric::new(
                name,
                kind,
                value,
            )
            .with_tags(Some(tags)),
        )
    }

    #[test]
    fn incremental() {
        let mut agg = Aggregate::new(&AggregateConfig { interval_ms: Some(1000_u64) }).unwrap();

        let tags: BTreeMap<String, String> =
            vec![("tag1".into(), "val1".into())].into_iter().collect();
        let counter_a_1 = make_metric("counter_a", metric::MetricKind::Incremental,
            metric::MetricValue::Counter { value: 42.0 }, tags.clone());
        let counter_a_2 = make_metric("counter_a", metric::MetricKind::Incremental,
            metric::MetricValue::Counter { value: 43.0 }, tags.clone());
        let counter_a_summed = make_metric("counter_a", metric::MetricKind::Incremental,
            metric::MetricValue::Counter { value: 85.0 }, tags.clone());

        // Single item, just stored regardless of kind
        agg.record(counter_a_1.clone());
        let mut out = vec![];
        // We should flush 1 item counter_a_1
        assert_eq!(1, agg.flush_into(&mut out));
        assert_eq!(1, out.len());
        assert_eq!(&counter_a_1, &out[0]);

        // A subsequent flush doesn't send out anything
        out.clear();
        assert_eq!(0, agg.flush_into(&mut out));
        assert_eq!(0, out.len());

        // One more just to make sure that we don't re-see from the other buffer
        out.clear();
        assert_eq!(0, agg.flush_into(&mut out));
        assert_eq!(0, out.len());

        // Two increments with the same series, should sum into 1
        agg.record(counter_a_1.clone());
        agg.record(counter_a_2.clone());
        out.clear();
        assert_eq!(1, agg.flush_into(&mut out));
        assert_eq!(1, out.len());
        assert_eq!(&counter_a_summed, &out[0]);

        let counter_b_1 = make_metric("counter_b", metric::MetricKind::Incremental,
            metric::MetricValue::Counter { value: 44.0 }, tags.clone());
        // Two increments with the different series, should get each back as-is
        agg.record(counter_a_1.clone());
        agg.record(counter_b_1.clone());
        out.clear();
        assert_eq!(2, agg.flush_into(&mut out));
        assert_eq!(2, out.len());
        // B/c we don't know the order they'll come back
        for event in out {
            match event.as_metric().series().name.name.as_str() {
                "counter_a" => assert_eq!(counter_a_1, event),
                "counter_b" => assert_eq!(counter_b_1, event),
                _ => assert!(false),
            }
        }
    }

    #[test]
    fn absolute() {
        let mut agg = Aggregate::new(&AggregateConfig { interval_ms: Some(1000_u64) }).unwrap();

        let tags: BTreeMap<String, String> =
            vec![("tag1".into(), "val1".into())].into_iter().collect();
        let gauge_a_1 = make_metric("gauge_a", metric::MetricKind::Absolute,
            metric::MetricValue::Gauge { value: 42.0 }, tags.clone());
        let gauge_a_2 = make_metric("gauge_a", metric::MetricKind::Absolute,
            metric::MetricValue::Gauge { value: 43.0 }, tags.clone());

        // Single item, just stored regardless of kind
        agg.record(gauge_a_1.clone());
        let mut out = vec![];
        // We should flush 1 item gauge_a_1
        assert_eq!(1, agg.flush_into(&mut out));
        assert_eq!(1, out.len());
        assert_eq!(&gauge_a_1, &out[0]);

        // A subsequent flush doesn't send out anything
        out.clear();
        assert_eq!(0, agg.flush_into(&mut out));
        assert_eq!(0, out.len());

        // One more just to make sure that we don't re-see from the other buffer
        out.clear();
        assert_eq!(0, agg.flush_into(&mut out));
        assert_eq!(0, out.len());

        // Two absolutes with the same series, should get the 2nd (last) back.
        agg.record(gauge_a_1.clone());
        agg.record(gauge_a_2.clone());
        out.clear();
        assert_eq!(1, agg.flush_into(&mut out));
        assert_eq!(1, out.len());
        assert_eq!(&gauge_a_2, &out[0]);

        let gauge_b_1 = make_metric("gauge_b", metric::MetricKind::Absolute,
            metric::MetricValue::Gauge { value: 44.0 }, tags.clone());
        // Two increments with the different series, should get each back as-is
        agg.record(gauge_a_1.clone());
        agg.record(gauge_b_1.clone());
        out.clear();
        assert_eq!(2, agg.flush_into(&mut out));
        assert_eq!(2, out.len());
        // B/c we don't know the order they'll come back
        for event in out {
            match event.as_metric().series().name.name.as_str() {
                "gauge_a" => assert_eq!(gauge_a_1, event),
                "gauge_b" => assert_eq!(gauge_b_1, event),
                _ => assert!(false),
            }
        }
    }

    #[tokio::test]
    async fn transform() {
        let agg = toml::from_str::<AggregateConfig>(
            r#"
interval_ms = 10
"#,
        )
        .unwrap()
        .build(&GlobalOptions::default())
        .await
        .unwrap();
        let agg = agg.into_task();

        let tags: BTreeMap<String, String> =
            vec![("tag1".into(), "val1".into())].into_iter().collect();
        let counter_a_1 = make_metric("counter_a", metric::MetricKind::Incremental,
            metric::MetricValue::Counter { value: 42.0 }, tags.clone());
        let counter_a_2 = make_metric("counter_a", metric::MetricKind::Incremental,
            metric::MetricValue::Counter { value: 43.0 }, tags.clone());
        let counter_a_summed = make_metric("counter_a", metric::MetricKind::Incremental,
            metric::MetricValue::Counter { value: 85.0 }, tags.clone());
        let gauge_a_1 = make_metric("gauge_a", metric::MetricKind::Absolute,
            metric::MetricValue::Gauge { value: 42.0 }, tags.clone());
        let gauge_a_2 = make_metric("gauge_a", metric::MetricKind::Absolute,
            metric::MetricValue::Gauge { value: 43.0 }, tags.clone());
        let inputs = vec![counter_a_1, counter_a_2, gauge_a_1, gauge_a_2.clone()];

        let in_stream = Box::pin(stream::iter(inputs));
        let mut out_stream = agg.transform(in_stream);

        let mut count = 0;
        while let Some(event) = out_stream.next().await {
            count += 1;
            match event.as_metric().series().name.name.as_str() {
                "counter_a" => assert_eq!(counter_a_summed, event),
                "gauge_a" => assert_eq!(gauge_a_2, event),
                _ => assert!(false),
            };
        }

        // There were only 2
        assert_eq!(2, count);
    }
}
