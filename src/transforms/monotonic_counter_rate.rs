use crate::{
    config::{DataType, TransformConfig, TransformDescription},
    event::{Event, Metric, MetricKind, MetricValue},
    internal_events::{MonotonicCounterRateEventConverted, MonotonicCounterRateEventProcessed},
    transforms::{TaskTransform, Transform},
};
use futures01::Stream as Stream01;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::convert::TryFrom;
use std::hash::{Hash, Hasher};

#[derive(Clone, Default, Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct MonotonicCounterRateConfig {}

inventory::submit! {
    TransformDescription::new::<MonotonicCounterRateConfig>("monotonic_counter_rate")
}

impl_generate_config_from_default!(MonotonicCounterRateConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "monotonic_counter_rate")]
impl TransformConfig for MonotonicCounterRateConfig {
    async fn build(&self) -> crate::Result<Transform> {
        Ok(Transform::task(MonotonicCounterRate::new()))
    }

    fn input_type(&self) -> DataType {
        DataType::Metric
    }

    fn output_type(&self) -> DataType {
        DataType::Metric
    }

    fn transform_type(&self) -> &'static str {
        "monotonic_counter_rate"
    }
}

#[derive(Debug)]
pub struct MonotonicCounterRate {
    cache: HashSet<MonotonicCounter>,
}

impl MonotonicCounterRate {
    fn new() -> Self {
        Self {
            cache: HashSet::new(),
        }
    }

    fn transform_one(&mut self, event: Event) -> Option<Event> {
        emit!(MonotonicCounterRateEventProcessed);
        match MonotonicCounter::try_from(event) {
            Ok(metric) => {
                let event = self
                    .cache
                    .get(&metric)
                    .and_then(|prev| {
                        prev.to_rate(&metric).map(|rate| {
                            emit!(MonotonicCounterRateEventConverted);
                            rate.into()
                        })
                    })
                    .or_else(|| Some(metric.to_event()));
                self.cache.replace(metric);
                event
            }
            Err(event) => Some(event),
        }
    }
}

impl TaskTransform for MonotonicCounterRate {
    fn transform(
        mut self: Box<Self>,
        task: Box<dyn Stream01<Item = Event, Error = ()> + Send>,
    ) -> Box<dyn Stream01<Item = Event, Error = ()> + Send>
    where
        Self: 'static,
    {
        Box::new(task.filter_map(move |v| self.transform_one(v)))
    }
}

#[derive(Debug)]
struct MonotonicCounter(Metric);

impl TryFrom<Event> for MonotonicCounter {
    type Error = Event;

    fn try_from(value: Event) -> Result<Self, Self::Error> {
        let metric = value.into_metric();
        match (&metric.kind, &metric.value) {
            (&MetricKind::Absolute, &MetricValue::Counter { .. }) => Ok(MonotonicCounter(metric)),
            (&MetricKind::Absolute, &MetricValue::AggregatedHistogram { .. }) => {
                Ok(MonotonicCounter(metric))
            }
            _ => Err(metric.into()),
        }
    }
}

impl PartialEq for MonotonicCounter {
    fn eq(&self, other: &Self) -> bool {
        self.0.name == other.0.name
            && self.0.namespace == other.0.namespace
            && self.0.tags == other.0.tags
    }
}

impl Eq for MonotonicCounter {}

impl Hash for MonotonicCounter {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.name.hash(state);
        self.0.namespace.hash(state);
        self.0.tags.hash(state);
    }
}

impl MonotonicCounter {
    fn to_rate(&self, next: &Self) -> Option<Metric> {
        let rate = match (&self.0.value, &next.0.value) {
            (MetricValue::Counter { value }, MetricValue::Counter { value: value2 }) => {
                // counter has been reset if the next value is smaller
                if value <= value2 {
                    Some(MetricValue::Counter {
                        value: value2 - value,
                    })
                } else {
                    None
                }
            }
            (
                MetricValue::AggregatedHistogram {
                    buckets,
                    counts,
                    count,
                    sum,
                },
                MetricValue::AggregatedHistogram {
                    buckets: buckets2,
                    counts: counts2,
                    count: count2,
                    sum: sum2,
                },
            ) => {
                // histogram has been reset if the next count is smaller
                if count <= count2 && buckets == buckets2 && counts.len() == counts2.len() {
                    let diff = counts
                        .iter()
                        .zip(counts2.iter())
                        .map(|(v, v2)| v2 - v)
                        .collect::<Vec<_>>();
                    Some(MetricValue::AggregatedHistogram {
                        buckets: buckets2.clone(),
                        counts: diff,
                        count: count2 - count,
                        sum: sum2 - sum,
                    })
                } else {
                    None
                }
            }
            _ => None,
        };

        rate.map(|value| Metric {
            name: next.0.name.clone(),
            namespace: next.0.namespace.clone(),
            timestamp: next.0.timestamp,
            tags: next.0.tags.clone(),
            kind: MetricKind::Incremental,
            value,
        })
    }

    fn to_event(&self) -> Event {
        Event::Metric(self.0.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, TimeZone, Utc};
    use std::iter;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<MonotonicCounterRateConfig>();
    }

    fn ts(minute: u32) -> DateTime<Utc> {
        Utc.ymd(2018, 11, 14).and_hms_nano(8, minute, 0, 0)
    }

    fn make_counter(minute: u32, value: f64) -> Event {
        Event::Metric(Metric {
            name: "counter".into(),
            namespace: None,
            timestamp: Some(ts(minute)),
            tags: Some(vec![("host".into(), "local".into())].into_iter().collect()),
            kind: MetricKind::Absolute,
            value: MetricValue::Counter { value },
        })
    }

    #[test]
    fn counter_rate() {
        let mut transform = MonotonicCounterRate::new();

        let event1 = transform.transform_one(make_counter(0, 10.0));
        let event2 = transform.transform_one(make_counter(1, 20.0));
        let event3 = transform.transform_one(make_counter(2, 40.0));

        assert_eq!(
            event1,
            Some(Event::Metric(Metric {
                name: "counter".into(),
                namespace: None,
                timestamp: Some(ts(0)),
                tags: Some(vec![("host".into(), "local".into())].into_iter().collect()),
                kind: MetricKind::Absolute,
                value: MetricValue::Counter { value: 10.0 },
            }))
        );
        assert_eq!(
            event2,
            Some(Event::Metric(Metric {
                name: "counter".into(),
                namespace: None,
                timestamp: Some(ts(1)),
                tags: Some(vec![("host".into(), "local".into())].into_iter().collect()),
                kind: MetricKind::Incremental,
                value: MetricValue::Counter { value: 10.0 },
            }))
        );
        assert_eq!(
            event3,
            Some(Event::Metric(Metric {
                name: "counter".into(),
                namespace: None,
                timestamp: Some(ts(2)),
                tags: Some(vec![("host".into(), "local".into())].into_iter().collect()),
                kind: MetricKind::Incremental,
                value: MetricValue::Counter { value: 20.0 },
            }))
        );
    }

    #[test]
    fn counter_rate_reset() {
        let mut transform = MonotonicCounterRate::new();

        let event1 = transform.transform_one(make_counter(0, 10.0));
        let event2 = transform.transform_one(make_counter(1, 20.0));
        let event3 = transform.transform_one(make_counter(2, 10.0));
        let event4 = transform.transform_one(make_counter(3, 40.0));

        assert_eq!(
            event1,
            Some(Event::Metric(Metric {
                name: "counter".into(),
                namespace: None,
                timestamp: Some(ts(0)),
                tags: Some(vec![("host".into(), "local".into())].into_iter().collect()),
                kind: MetricKind::Absolute,
                value: MetricValue::Counter { value: 10.0 },
            }))
        );
        assert_eq!(
            event2,
            Some(Event::Metric(Metric {
                name: "counter".into(),
                namespace: None,
                timestamp: Some(ts(1)),
                tags: Some(vec![("host".into(), "local".into())].into_iter().collect()),
                kind: MetricKind::Incremental,
                value: MetricValue::Counter { value: 10.0 },
            }))
        );
        assert_eq!(
            event3,
            Some(Event::Metric(Metric {
                name: "counter".into(),
                namespace: None,
                timestamp: Some(ts(2)),
                tags: Some(vec![("host".into(), "local".into())].into_iter().collect()),
                kind: MetricKind::Absolute,
                value: MetricValue::Counter { value: 10.0 },
            }))
        );
        assert_eq!(
            event4,
            Some(Event::Metric(Metric {
                name: "counter".into(),
                namespace: None,
                timestamp: Some(ts(3)),
                tags: Some(vec![("host".into(), "local".into())].into_iter().collect()),
                kind: MetricKind::Incremental,
                value: MetricValue::Counter { value: 30.0 },
            }))
        );
    }

    fn make_histogram(minute: u32, count: u32) -> Event {
        Event::Metric(Metric {
            name: "histogram".into(),
            namespace: Some("host".into()),
            timestamp: Some(ts(minute)),
            tags: None,
            kind: MetricKind::Absolute,
            value: MetricValue::AggregatedHistogram {
                buckets: vec![1.0, 10.0, 100.0],
                counts: iter::repeat(count).take(3).collect(),
                count: count * 3,
                sum: (111 * count) as f64,
            },
        })
    }

    #[test]
    fn histogram_rate() {
        let mut transform = MonotonicCounterRate::new();

        let event1 = transform.transform_one(make_histogram(0, 1));
        let event2 = transform.transform_one(make_histogram(1, 2));
        let event3 = transform.transform_one(make_histogram(2, 4));

        assert_eq!(
            event1,
            Some(Event::Metric(Metric {
                name: "histogram".into(),
                namespace: Some("host".into()),
                timestamp: Some(ts(0)),
                tags: None,
                kind: MetricKind::Absolute,
                value: MetricValue::AggregatedHistogram {
                    buckets: vec![1.0, 10.0, 100.0],
                    counts: iter::repeat(1).take(3).collect(),
                    count: 3,
                    sum: 111.0,
                },
            }))
        );
        assert_eq!(
            event2,
            Some(Event::Metric(Metric {
                name: "histogram".into(),
                namespace: Some("host".into()),
                timestamp: Some(ts(1)),
                tags: None,
                kind: MetricKind::Incremental,
                value: MetricValue::AggregatedHistogram {
                    buckets: vec![1.0, 10.0, 100.0],
                    counts: iter::repeat(1).take(3).collect(),
                    count: 3,
                    sum: 111.0,
                },
            }))
        );
        assert_eq!(
            event3,
            Some(Event::Metric(Metric {
                name: "histogram".into(),
                namespace: Some("host".into()),
                timestamp: Some(ts(2)),
                tags: None,
                kind: MetricKind::Incremental,
                value: MetricValue::AggregatedHistogram {
                    buckets: vec![1.0, 10.0, 100.0],
                    counts: iter::repeat(2).take(3).collect(),
                    count: 6,
                    sum: 222.0,
                },
            }))
        );
    }

    #[test]
    fn histogram_rate_reset() {
        let mut transform = MonotonicCounterRate::new();

        let event1 = transform.transform_one(make_histogram(0, 1));
        let event2 = transform.transform_one(make_histogram(1, 2));
        let event3 = transform.transform_one(make_histogram(2, 1));
        let event4 = transform.transform_one(make_histogram(3, 4));

        assert_eq!(
            event1,
            Some(Event::Metric(Metric {
                name: "histogram".into(),
                namespace: Some("host".into()),
                timestamp: Some(ts(0)),
                tags: None,
                kind: MetricKind::Absolute,
                value: MetricValue::AggregatedHistogram {
                    buckets: vec![1.0, 10.0, 100.0],
                    counts: iter::repeat(1).take(3).collect(),
                    count: 3,
                    sum: 111.0,
                },
            }))
        );
        assert_eq!(
            event2,
            Some(Event::Metric(Metric {
                name: "histogram".into(),
                namespace: Some("host".into()),
                timestamp: Some(ts(1)),
                tags: None,
                kind: MetricKind::Incremental,
                value: MetricValue::AggregatedHistogram {
                    buckets: vec![1.0, 10.0, 100.0],
                    counts: iter::repeat(1).take(3).collect(),
                    count: 3,
                    sum: 111.0,
                },
            }))
        );
        assert_eq!(
            event3,
            Some(Event::Metric(Metric {
                name: "histogram".into(),
                namespace: Some("host".into()),
                timestamp: Some(ts(2)),
                tags: None,
                kind: MetricKind::Absolute,
                value: MetricValue::AggregatedHistogram {
                    buckets: vec![1.0, 10.0, 100.0],
                    counts: iter::repeat(1).take(3).collect(),
                    count: 3,
                    sum: 111.0,
                },
            }))
        );
        assert_eq!(
            event4,
            Some(Event::Metric(Metric {
                name: "histogram".into(),
                namespace: Some("host".into()),
                timestamp: Some(ts(3)),
                tags: None,
                kind: MetricKind::Incremental,
                value: MetricValue::AggregatedHistogram {
                    buckets: vec![1.0, 10.0, 100.0],
                    counts: iter::repeat(3).take(3).collect(),
                    count: 9,
                    sum: 333.0,
                },
            }))
        );
    }

    fn make_gauge(minute: u32, value: f64) -> Event {
        Event::Metric(Metric {
            name: "gauge".into(),
            namespace: None,
            timestamp: Some(ts(minute)),
            tags: None,
            kind: MetricKind::Absolute,
            value: MetricValue::Gauge { value },
        })
    }

    #[test]
    fn skip_gauge() {
        let mut transform = MonotonicCounterRate::new();

        let event1 = transform.transform_one(make_gauge(0, 10.0));
        let event2 = transform.transform_one(make_gauge(1, 20.0));

        assert_eq!(
            event1,
            Some(Event::Metric(Metric {
                name: "gauge".into(),
                namespace: None,
                timestamp: Some(ts(0)),
                tags: None,
                kind: MetricKind::Absolute,
                value: MetricValue::Gauge { value: 10.0 },
            }))
        );
        assert_eq!(
            event2,
            Some(Event::Metric(Metric {
                name: "gauge".into(),
                namespace: None,
                timestamp: Some(ts(1)),
                tags: None,
                kind: MetricKind::Absolute,
                value: MetricValue::Gauge { value: 20.0 },
            }))
        );
    }
}
