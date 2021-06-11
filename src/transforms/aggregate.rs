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
                    Some(existing) => existing.add(data),
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

    fn flush_into(&mut self, output: &mut Vec<Event>) {
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

        if map.len() == 0 {
            return
        }

        // TODO: not clear how this should work with aggregation so just stuffing a default one in
        // for now.
        let metadata = EventMetadata::default();

        for (series, metric) in map.drain() {
            let metric = metric::Metric::from_parts(series, metric, metadata.clone());
            output.push(Event::Metric(metric));
        }

        emit!(AggregateFlushed);
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
    /*
    use super::*;
    use crate::{event::metric, event::Event, event::Metric};
    use std::collections::BTreeMap;

    #[test]
    fn genreate_config() {
        crate::test_util::test_generate_config::<AggregateConfig>();
    }
    */

    #[test]
    fn counters() {
        /*
        let mut agg = Aggregate::new(Duration::from_millis(10 * 1000));

        let counter_a = metric::MetricValue::Counter { value: 42.0 };
        let counter_b = metric::MetricValue::Counter { value: 43.0 };
        let summed = metric::MetricValue::Counter { value: 85.0 };
        let tags: BTreeMap<String, String> =
            vec![("tag1".into(), "val1".into())].into_iter().collect();

        // Single item, just stored regardless of kind
        agg.record(make_metric("counter", metric::MetricKind::Incremental,
                counter_a.clone(), tags.clone()));
        assert_eq!(1, agg.map.len());
        match agg.map.values().next() {
            Some(record) => assert_eq!(counter_a, record.value),
            _ => assert!(false),
        }

        // When sent absolute, replaced, not incremented
        agg.record(make_metric("counter", metric::MetricKind::Absolute,
                counter_b.clone(), tags.clone()));
        assert_eq!(1, agg.map.len());
        match agg.map.values().next() {
            Some(record) => assert_eq!(counter_b, record.value),
            _ => assert!(false),
        }

        // Now back to incremental, expect them to be added
        agg.record(make_metric("counter", metric::MetricKind::Incremental,
                counter_a.clone(), tags.clone()));
        assert_eq!(1, agg.map.len());
        match agg.map.values().next() {
            Some(record) => assert_eq!(summed, record.value),
            _ => assert!(false),
        };

        // Different name should create a distinct entry
        agg.record(make_metric("counter2", metric::MetricKind::Incremental,
                counter_a.clone(), tags.clone()));
        assert_eq!(2, agg.map.len());
        for (key, record) in &agg.map {
            match key.name.name.as_str() {
                "counter" => assert_eq!(summed, record.value),
                "counter2" => assert_eq!(counter_a, record.value),
                _ => assert!(false),
            }
        }

        // Different MetricValue type, guage, with same name & tags is ignored, first establishes
        // type
        let guage = metric::MetricValue::Gauge { value: 44.0 };
        agg.record(make_metric("counter", metric::MetricKind::Incremental,
                guage.clone(), tags.clone()));
        // Nothing changed
        assert_eq!(2, agg.map.len());
        for (key, record) in &agg.map {
            match key.name.name.as_str() {
                "counter" => assert_eq!(summed, record.value),
                "counter2" => assert_eq!(counter_a, record.value),
                _ => assert!(false),
            }
        }
        */
    }

    /*
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
    */

    /*
    use super::*;
    use crate::{
        conditions::check_fields::CheckFieldsPredicateArg, config::log_schema, event::Event,
        test_util::random_lines, transforms::test::transform_one,
    };
    use approx::assert_relative_eq;
    use indexmap::IndexMap;

    fn condition_contains(pre: &str) -> Box<dyn Condition> {
        condition(log_schema().message_key(), "contains", pre)
    }

    fn condition(field: &str, condition: &str, value: &str) -> Box<dyn Condition> {
        let mut preds: IndexMap<String, CheckFieldsPredicateArg> = IndexMap::new();
        preds.insert(
            format!("{}.{}", field, condition),
            CheckFieldsPredicateArg::String(value.into()),
        );

        CheckFieldsConfig::new(preds).build().unwrap()
    }

    #[test]
    fn genreate_config() {
        crate::test_util::test_generate_config::<AggregateConfig>();
    }

    #[test]
    fn hash_samples_at_roughly_the_configured_rate() {
        let num_events = 10000;

        let events = random_events(num_events);
        let mut sampler = Aggregate::new(
            2,
            Some(log_schema().message_key().into()),
            Some(condition_contains("na")),
        );
        let total_passed = events
            .into_iter()
            .filter_map(|event| {
                let mut buf = Vec::with_capacity(1);
                sampler.transform(&mut buf, event);
                buf.pop()
            })
            .count();
        let ideal = 1.0f64 / 2.0f64;
        let actual = total_passed as f64 / num_events as f64;
        assert_relative_eq!(ideal, actual, epsilon = ideal * 0.5);

        let events = random_events(num_events);
        let mut sampler = Aggregate::new(
            25,
            Some(log_schema().message_key().into()),
            Some(condition_contains("na")),
        );
        let total_passed = events
            .into_iter()
            .filter_map(|event| {
                let mut buf = Vec::with_capacity(1);
                sampler.transform(&mut buf, event);
                buf.pop()
            })
            .count();
        let ideal = 1.0f64 / 25.0f64;
        let actual = total_passed as f64 / num_events as f64;
        assert_relative_eq!(ideal, actual, epsilon = ideal * 0.5);
    }

    #[test]
    fn hash_consistently_samples_the_same_events() {
        let events = random_events(1000);
        let mut sampler = Aggregate::new(
            2,
            Some(log_schema().message_key().into()),
            Some(condition_contains("na")),
        );

        let first_run = events
            .clone()
            .into_iter()
            .filter_map(|event| {
                let mut buf = Vec::with_capacity(1);
                sampler.transform(&mut buf, event);
                buf.pop()
            })
            .collect::<Vec<_>>();
        let second_run = events
            .into_iter()
            .filter_map(|event| {
                let mut buf = Vec::with_capacity(1);
                sampler.transform(&mut buf, event);
                buf.pop()
            })
            .collect::<Vec<_>>();

        assert_eq!(first_run, second_run);
    }

    #[test]
    fn always_passes_events_matching_pass_list() {
        for key_field in &[None, Some(log_schema().message_key().into())] {
            let event = Event::from("i am important");
            let mut sampler =
                Aggregate::new(0, key_field.clone(), Some(condition_contains("important")));
            let iterations = 0..1000;
            let total_passed = iterations
                .filter_map(|_| {
                    transform_one(&mut sampler, event.clone())
                        .map(|result| assert_eq!(result, event))
                })
                .count();
            assert_eq!(total_passed, 1000);
        }
    }

    #[test]
    fn handles_key_field() {
        for key_field in &[None, Some(log_schema().timestamp_key().into())] {
            let event = Event::from("nananana");
            let mut sampler = Aggregate::new(
                0,
                key_field.clone(),
                Some(condition(log_schema().timestamp_key(), "contains", ":")),
            );
            let iterations = 0..1000;
            let total_passed = iterations
                .filter_map(|_| {
                    transform_one(&mut sampler, event.clone())
                        .map(|result| assert_eq!(result, event))
                })
                .count();
            assert_eq!(total_passed, 1000);
        }
    }

    #[test]
    fn sampler_adds_sampling_rate_to_event() {
        for key_field in &[None, Some(log_schema().message_key().into())] {
            let events = random_events(10000);
            let mut sampler = Aggregate::new(10, key_field.clone(), Some(condition_contains("na")));
            let passing = events
                .into_iter()
                .filter(|s| {
                    !s.as_log()[log_schema().message_key()]
                        .to_string_lossy()
                        .contains("na")
                })
                .find_map(|event| transform_one(&mut sampler, event))
                .unwrap();
            assert_eq!(passing.as_log()["sample_rate"], "10".into());

            let events = random_events(10000);
            let mut sampler = Aggregate::new(25, key_field.clone(), Some(condition_contains("na")));
            let passing = events
                .into_iter()
                .filter(|s| {
                    !s.as_log()[log_schema().message_key()]
                        .to_string_lossy()
                        .contains("na")
                })
                .find_map(|event| transform_one(&mut sampler, event))
                .unwrap();
            assert_eq!(passing.as_log()["sample_rate"], "25".into());

            // If the event passed the regex check, don't include the sampling rate
            let mut sampler = Aggregate::new(25, key_field.clone(), Some(condition_contains("na")));
            let event = Event::from("nananana");
            let passing = transform_one(&mut sampler, event).unwrap();
            assert!(passing.as_log().get("sample_rate").is_none());
        }
    }

    fn random_events(n: usize) -> Vec<Event> {
        random_lines(10).take(n).map(Event::from).collect()
    }
    */
}
