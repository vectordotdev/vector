use super::batch::Batch;
use crate::event::{metric::Direction, Event, Metric};
use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use ordered_float::OrderedFloat;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct MetricKey {
    name: String,
    tags: Option<Vec<(String, String)>>,
}

impl MetricKey {
    fn new(name: String, tags: Option<HashMap<String, String>>) -> Self {
        Self {
            name,
            tags: tags.map(|m| m.into_iter().collect()),
        }
    }

    fn tags(&self) -> Option<HashMap<String, String>> {
        self.tags.clone().map(|m| m.into_iter().collect())
    }

    fn name(&self) -> String {
        self.name.clone()
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
struct AggregatedMetric<T> {
    val: f64,
    vals: Vec<T>,
    timestamp: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MetricBuffer {
    num_items: usize,
    counters: IndexMap<MetricKey, AggregatedMetric<f64>>,
    gauges: IndexMap<MetricKey, AggregatedMetric<f64>>,
    sets: IndexMap<MetricKey, AggregatedMetric<String>>,
    histograms: IndexMap<MetricKey, AggregatedMetric<f64>>,
}

impl MetricBuffer {
    pub fn new() -> Self {
        Self {
            num_items: 0,
            counters: IndexMap::new(),
            gauges: IndexMap::new(),
            sets: IndexMap::new(),
            histograms: IndexMap::new(),
        }
    }
}

impl Batch for MetricBuffer {
    type Input = Event;
    type Output = Vec<Event>;

    fn len(&self) -> usize {
        self.num_items()
    }

    fn push(&mut self, item: Self::Input) {
        self.num_items += 1;
        match item.into_metric() {
            Metric::Counter {
                name,
                val,
                timestamp,
                tags,
            } => {
                let key = MetricKey::new(name, tags);
                let mut counter = self.counters.entry(key).or_default();
                counter.val += val;
                counter.timestamp = timestamp;
            }
            Metric::Gauge {
                name,
                val,
                direction,
                timestamp,
                tags,
            } => {
                let key = MetricKey::new(name, tags);
                let mut gauge = self.gauges.entry(key).or_default();

                if direction.is_none() {
                    gauge.val = val;
                } else {
                    let delta = match direction {
                        None => 0.0,
                        Some(Direction::Plus) => val,
                        Some(Direction::Minus) => -val,
                    };
                    gauge.val += delta;
                }
                gauge.timestamp = timestamp;
            }
            Metric::Set {
                name,
                val,
                timestamp,
                tags,
            } => {
                let key = MetricKey::new(name, tags);
                let mut set = self.sets.entry(key).or_default();
                set.vals.push(val);
                set.timestamp = timestamp;
            }
            Metric::Histogram {
                name,
                val,
                sample_rate,
                timestamp,
                tags,
            } => {
                let key = MetricKey::new(name, tags);
                let mut hist = self.histograms.entry(key).or_default();
                for _ in 0..sample_rate {
                    hist.vals.push(val);
                }
                hist.timestamp = timestamp;
            }
        }
    }

    fn is_empty(&self) -> bool {
        self.num_items() == 0
    }

    fn fresh(&self) -> Self {
        Self {
            num_items: 0,
            counters: IndexMap::new(),
            gauges: self.gauges.clone(),
            sets: IndexMap::new(),
            histograms: IndexMap::new(),
        }
    }

    fn finish(self) -> Self::Output {
        let counters = self.counters.into_iter().map(|(k, v)| {
            Event::Metric(Metric::Counter {
                name: k.name(),
                val: v.val,
                timestamp: v.timestamp,
                tags: k.tags(),
            })
        });

        let gauges = self.gauges.into_iter().map(|(k, v)| {
            Event::Metric(Metric::Gauge {
                name: k.name(),
                val: v.val,
                direction: None,
                timestamp: v.timestamp,
                tags: k.tags(),
            })
        });

        let sets = self.sets.into_iter().map(|(k, v)| {
            let set: HashSet<_> = v.vals.into_iter().collect();
            Event::Metric(Metric::Gauge {
                name: k.name(),
                val: set.len() as f64,
                direction: None,
                timestamp: v.timestamp,
                tags: k.tags(),
            })
        });

        let histograms = self.histograms.into_iter().flat_map(|(k, v)| {
            let mut sampled: IndexMap<OrderedFloat<f64>, u32> = IndexMap::new();
            for val in v.vals.iter() {
                sampled
                    .entry(OrderedFloat::from(*val))
                    .and_modify(|c| *c += 1)
                    .or_insert(1);
            }

            sampled
                .iter()
                .map(|(val, count)| {
                    Event::Metric(Metric::Histogram {
                        name: k.name(),
                        val: **val,
                        sample_rate: *count,
                        timestamp: v.timestamp,
                        tags: k.tags(),
                    })
                })
                .collect::<Vec<_>>()
        });

        counters
            .chain(gauges)
            .chain(sets)
            .chain(histograms)
            .collect()
    }

    fn num_items(&self) -> usize {
        self.num_items
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::sinks::util::batch::BatchSink;
    use crate::{event::metric::Metric, Event};
    use futures::{future::Future, stream, Sink};
    use pretty_assertions::assert_eq;
    use std::time::Duration;

    fn tag(name: &str) -> HashMap<String, String> {
        vec![(name.to_owned(), "true".to_owned())]
            .into_iter()
            .collect()
    }

    #[test]
    fn metric_buffer_counters() {
        let sink = BatchSink::new_min_max(
            vec![],
            MetricBuffer::new(),
            6,
            0,
            Some(Duration::from_secs(1)),
        );

        let mut events = Vec::new();
        for i in 0..4 {
            let event = Event::Metric(Metric::Counter {
                name: "counter-0".into(),
                val: i as f64,
                timestamp: None,
                tags: Some(tag("production")),
            });
            events.push(event);
        }

        for i in 0..4 {
            let event = Event::Metric(Metric::Counter {
                name: format!("counter-{}", i),
                val: i as f64,
                timestamp: None,
                tags: Some(tag("staging")),
            });
            events.push(event);
        }

        for i in 0..4 {
            let event = Event::Metric(Metric::Counter {
                name: format!("counter-{}", i),
                val: i as f64,
                timestamp: None,
                tags: Some(tag("production")),
            });
            events.push(event);
        }

        let (buffer, _) = sink
            .send_all(stream::iter_ok(events.into_iter()))
            .wait()
            .unwrap();

        let buffer = buffer.into_inner();
        assert_eq!(buffer.len(), 2);
        assert_eq!(buffer[0].len(), 6);
        assert_eq!(buffer[1].len(), 6);

        assert_eq!(
            buffer[0].clone().finish(),
            [
                Event::Metric(Metric::Counter {
                    name: "counter-0".into(),
                    val: 6.0,
                    timestamp: None,
                    tags: Some(tag("production")),
                }),
                Event::Metric(Metric::Counter {
                    name: "counter-0".into(),
                    val: 0.0,
                    timestamp: None,
                    tags: Some(tag("staging")),
                }),
                Event::Metric(Metric::Counter {
                    name: "counter-1".into(),
                    val: 1.0,
                    timestamp: None,
                    tags: Some(tag("staging")),
                }),
            ]
        );

        assert_eq!(
            buffer[1].clone().finish(),
            [
                Event::Metric(Metric::Counter {
                    name: "counter-2".into(),
                    val: 2.0,
                    timestamp: None,
                    tags: Some(tag("staging")),
                }),
                Event::Metric(Metric::Counter {
                    name: "counter-3".into(),
                    val: 3.0,
                    timestamp: None,
                    tags: Some(tag("staging")),
                }),
                Event::Metric(Metric::Counter {
                    name: "counter-0".into(),
                    val: 0.0,
                    timestamp: None,
                    tags: Some(tag("production")),
                }),
                Event::Metric(Metric::Counter {
                    name: "counter-1".into(),
                    val: 1.0,
                    timestamp: None,
                    tags: Some(tag("production")),
                }),
                Event::Metric(Metric::Counter {
                    name: "counter-2".into(),
                    val: 2.0,
                    timestamp: None,
                    tags: Some(tag("production")),
                }),
                Event::Metric(Metric::Counter {
                    name: "counter-3".into(),
                    val: 3.0,
                    timestamp: None,
                    tags: Some(tag("production")),
                }),
            ]
        );
    }

    #[test]
    fn metric_buffer_histograms() {
        let sink = BatchSink::new_min_max(
            vec![],
            MetricBuffer::new(),
            6,
            0,
            Some(Duration::from_secs(1)),
        );

        let mut events = Vec::new();
        for i in 0..3 {
            let event = Event::Metric(Metric::Histogram {
                name: "hist-0".into(),
                val: i as f64,
                sample_rate: 10,
                timestamp: None,
                tags: None,
            });
            events.push(event);
        }

        for i in 0..10 {
            let event = Event::Metric(Metric::Histogram {
                name: format!("hist-{}", i),
                val: i as f64,
                sample_rate: 1,
                timestamp: None,
                tags: None,
            });
            events.push(event);
        }

        let (buffer, _) = sink
            .send_all(stream::iter_ok(events.into_iter()))
            .wait()
            .unwrap();

        let buffer = buffer.into_inner();
        assert_eq!(buffer.len(), 3);
        assert_eq!(buffer[0].len(), 6);
        assert_eq!(buffer[1].len(), 6);
        assert_eq!(buffer[2].len(), 1);

        assert_eq!(
            buffer[0].clone().finish(),
            [
                Event::Metric(Metric::Histogram {
                    name: "hist-0".into(),
                    val: 0.0,
                    sample_rate: 11,
                    timestamp: None,
                    tags: None,
                }),
                Event::Metric(Metric::Histogram {
                    name: "hist-0".into(),
                    val: 1.0,
                    sample_rate: 10,
                    timestamp: None,
                    tags: None,
                }),
                Event::Metric(Metric::Histogram {
                    name: "hist-0".into(),
                    val: 2.0,
                    sample_rate: 10,
                    timestamp: None,
                    tags: None,
                }),
                Event::Metric(Metric::Histogram {
                    name: "hist-1".into(),
                    val: 1.0,
                    sample_rate: 1,
                    timestamp: None,
                    tags: None,
                }),
                Event::Metric(Metric::Histogram {
                    name: "hist-2".into(),
                    val: 2.0,
                    sample_rate: 1,
                    timestamp: None,
                    tags: None,
                }),
            ]
        );

        assert_eq!(
            buffer[1].clone().finish(),
            [
                Event::Metric(Metric::Histogram {
                    name: "hist-3".into(),
                    val: 3.0,
                    sample_rate: 1,
                    timestamp: None,
                    tags: None,
                }),
                Event::Metric(Metric::Histogram {
                    name: "hist-4".into(),
                    val: 4.0,
                    sample_rate: 1,
                    timestamp: None,
                    tags: None,
                }),
                Event::Metric(Metric::Histogram {
                    name: "hist-5".into(),
                    val: 5.0,
                    sample_rate: 1,
                    timestamp: None,
                    tags: None,
                }),
                Event::Metric(Metric::Histogram {
                    name: "hist-6".into(),
                    val: 6.0,
                    sample_rate: 1,
                    timestamp: None,
                    tags: None,
                }),
                Event::Metric(Metric::Histogram {
                    name: "hist-7".into(),
                    val: 7.0,
                    sample_rate: 1,
                    timestamp: None,
                    tags: None,
                }),
                Event::Metric(Metric::Histogram {
                    name: "hist-8".into(),
                    val: 8.0,
                    sample_rate: 1,
                    timestamp: None,
                    tags: None,
                }),
            ]
        );

        assert_eq!(
            buffer[2].clone().finish(),
            [Event::Metric(Metric::Histogram {
                name: "hist-9".into(),
                val: 9.0,
                sample_rate: 1,
                timestamp: None,
                tags: None,
            }),]
        );
    }
}
