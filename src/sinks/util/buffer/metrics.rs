use crate::event::{Event, Metric};
use crate::sinks::util::Batch;
use indexmap::IndexMap;

trait Aggregate {
    fn partition(&self) -> String;
}

impl Aggregate for Metric {
    fn partition(&self) -> String {
        match self {
            Metric::Counter { name, tags, .. } => format!("{}{:?}", name, tags),
            Metric::Gauge { name, tags, .. } => format!("{}{:?}", name, tags),
            Metric::Set {
                name, tags, val, ..
            } => format!("{}{:?}{}", name, tags, val),
            Metric::Histogram {
                name, tags, val, ..
            } => format!("{}{:?}{}", name, tags, val),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MetricBuffer {
    state: IndexMap<String, Metric>,
    metrics: IndexMap<String, Metric>,
}

impl MetricBuffer {
    pub fn new() -> Self {
        Self {
            state: IndexMap::new(),
            metrics: IndexMap::new(),
        }
    }
}

impl Batch for MetricBuffer {
    type Input = Event;
    type Output = Vec<Metric>;

    fn len(&self) -> usize {
        self.num_items()
    }

    fn push(&mut self, item: Self::Input) {
        let item = item.into_metric();
        let key = item.partition();

        match item {
            Metric::Counter { .. } => {
                if let Some(metric) = self.metrics.get_mut(&key) {
                    metric.merge(&item);
                } else {
                    self.metrics.insert(key, item);
                }
            }
            Metric::Gauge { .. } => {
                if let Some(metric) = self.metrics.get_mut(&key) {
                    metric.merge(&item);
                } else {
                    // if the gauge is not present in active batch,
                    // then we look it up in permanent state, where we keep track
                    // of gauge values throughout the entire application uptime
                    let value = if let Some(default) = self.state.get(&key) {
                        default.clone()
                    } else {
                        Metric::Gauge {
                            name: String::from(""),
                            val: 0.0,
                            direction: None,
                            timestamp: None,
                            tags: None,
                        }
                    };
                    self.metrics.entry(key).or_insert(value).merge(&item);
                }
            }
            Metric::Set { .. } => {
                self.metrics.insert(key, item);
            }
            Metric::Histogram { .. } => {
                if let Some(metric) = self.metrics.get_mut(&key) {
                    metric.merge(&item);
                } else {
                    self.metrics.insert(key, item);
                }
            }
        }
    }

    fn is_empty(&self) -> bool {
        self.num_items() == 0
    }

    fn fresh(&self) -> Self {
        let mut state = self.state.clone();
        for (k, v) in self.metrics.iter() {
            if v.is_gauge() {
                state.insert(k.clone(), v.clone());
            }
        }

        Self {
            state,
            metrics: IndexMap::new(),
        }
    }

    fn finish(self) -> Self::Output {
        self.metrics.values().cloned().collect()
    }

    fn num_items(&self) -> usize {
        self.metrics.len()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::sinks::util::batch::BatchSink;
    use crate::{
        event::metric::{Direction, Metric},
        Event,
    };
    use futures::{future::Future, stream, Sink};
    use pretty_assertions::assert_eq;
    use std::collections::HashMap;
    use std::time::Duration;

    fn tag(name: &str) -> HashMap<String, String> {
        vec![(name.to_owned(), "true".to_owned())]
            .into_iter()
            .collect()
    }

    #[test]
    fn metric_buffer_counters() {
        let sink = BatchSink::new_max(vec![], MetricBuffer::new(), 6, Some(Duration::from_secs(1)));

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
        assert_eq!(buffer[1].len(), 2);

        assert_eq!(
            buffer[0].clone().finish(),
            [
                Metric::Counter {
                    name: "counter-0".into(),
                    val: 6.0,
                    timestamp: None,
                    tags: Some(tag("production")),
                },
                Metric::Counter {
                    name: "counter-0".into(),
                    val: 0.0,
                    timestamp: None,
                    tags: Some(tag("staging")),
                },
                Metric::Counter {
                    name: "counter-1".into(),
                    val: 1.0,
                    timestamp: None,
                    tags: Some(tag("staging")),
                },
                Metric::Counter {
                    name: "counter-2".into(),
                    val: 2.0,
                    timestamp: None,
                    tags: Some(tag("staging")),
                },
                Metric::Counter {
                    name: "counter-3".into(),
                    val: 3.0,
                    timestamp: None,
                    tags: Some(tag("staging")),
                },
                Metric::Counter {
                    name: "counter-1".into(),
                    val: 1.0,
                    timestamp: None,
                    tags: Some(tag("production")),
                },
            ]
        );

        assert_eq!(
            buffer[1].clone().finish(),
            [
                Metric::Counter {
                    name: "counter-2".into(),
                    val: 2.0,
                    timestamp: None,
                    tags: Some(tag("production")),
                },
                Metric::Counter {
                    name: "counter-3".into(),
                    val: 3.0,
                    timestamp: None,
                    tags: Some(tag("production")),
                },
            ]
        );
    }

    #[test]
    fn metric_buffer_gauges() {
        let sink = BatchSink::new_max(vec![], MetricBuffer::new(), 4, Some(Duration::from_secs(1)));

        let mut events = Vec::new();
        for i in 0..4 {
            let event = Event::Metric(Metric::Gauge {
                name: "gauge-0".into(),
                val: i as f64,
                direction: None,
                timestamp: None,
                tags: Some(tag("production")),
            });
            events.push(event);
        }

        for i in 0..5 {
            let event = Event::Metric(Metric::Gauge {
                name: format!("gauge-{}", i),
                val: i as f64,
                direction: None,
                timestamp: None,
                tags: Some(tag("staging")),
            });
            events.push(event);
        }

        for i in 0..5 {
            let event = Event::Metric(Metric::Gauge {
                name: format!("gauge-{}", i),
                val: i as f64,
                direction: Some(Direction::Plus),
                timestamp: None,
                tags: Some(tag("staging")),
            });
            events.push(event);
        }

        let (buffer, _) = sink
            .send_all(stream::iter_ok(events.into_iter()))
            .wait()
            .unwrap();

        let buffer = buffer.into_inner();
        assert_eq!(buffer.len(), 3);
        assert_eq!(buffer[0].len(), 4);
        assert_eq!(buffer[1].len(), 4);
        assert_eq!(buffer[2].len(), 3);

        assert_eq!(
            buffer[0].clone().finish(),
            [
                Metric::Gauge {
                    name: "gauge-0".into(),
                    val: 3.0,
                    direction: None,
                    timestamp: None,
                    tags: Some(tag("production")),
                },
                Metric::Gauge {
                    name: "gauge-0".into(),
                    val: 0.0,
                    direction: None,
                    timestamp: None,
                    tags: Some(tag("staging")),
                },
                Metric::Gauge {
                    name: "gauge-1".into(),
                    val: 1.0,
                    direction: None,
                    timestamp: None,
                    tags: Some(tag("staging")),
                },
                Metric::Gauge {
                    name: "gauge-2".into(),
                    val: 2.0,
                    direction: None,
                    timestamp: None,
                    tags: Some(tag("staging")),
                },
            ]
        );

        assert_eq!(
            buffer[1].clone().finish(),
            [
                Metric::Gauge {
                    name: "gauge-3".into(),
                    val: 3.0,
                    direction: None,
                    timestamp: None,
                    tags: Some(tag("staging")),
                },
                Metric::Gauge {
                    name: "gauge-4".into(),
                    val: 4.0,
                    direction: None,
                    timestamp: None,
                    tags: Some(tag("staging")),
                },
                Metric::Gauge {
                    name: "gauge-0".into(),
                    val: 0.0,
                    direction: None,
                    timestamp: None,
                    tags: Some(tag("staging")),
                },
                Metric::Gauge {
                    name: "gauge-1".into(),
                    val: 1.0 + 1.0,
                    direction: None,
                    timestamp: None,
                    tags: Some(tag("staging")),
                },
            ]
        );

        assert_eq!(
            buffer[2].clone().finish(),
            [
                Metric::Gauge {
                    name: "gauge-2".into(),
                    val: 2.0 + 2.0,
                    direction: None,
                    timestamp: None,
                    tags: Some(tag("staging")),
                },
                Metric::Gauge {
                    name: "gauge-3".into(),
                    val: 3.0 + 3.0,
                    direction: None,
                    timestamp: None,
                    tags: Some(tag("staging")),
                },
                Metric::Gauge {
                    name: "gauge-4".into(),
                    val: 4.0 + 4.0,
                    direction: None,
                    timestamp: None,
                    tags: Some(tag("staging")),
                },
            ]
        );
    }

    #[test]
    fn metric_buffer_sets() {
        let sink = BatchSink::new_max(vec![], MetricBuffer::new(), 6, Some(Duration::from_secs(1)));

        let mut events = Vec::new();
        for i in 0..4 {
            let event = Event::Metric(Metric::Set {
                name: "set-0".into(),
                val: format!("{}", i),
                timestamp: None,
                tags: Some(tag("production")),
            });
            events.push(event);
        }

        for i in 0..4 {
            let event = Event::Metric(Metric::Set {
                name: "set-0".into(),
                val: format!("{}", i),
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
        assert_eq!(buffer.len(), 1);

        assert_eq!(
            buffer[0].clone().finish(),
            [
                Metric::Set {
                    name: "set-0".into(),
                    val: "0".into(),
                    timestamp: None,
                    tags: Some(tag("production")),
                },
                Metric::Set {
                    name: "set-0".into(),
                    val: "1".into(),
                    timestamp: None,
                    tags: Some(tag("production")),
                },
                Metric::Set {
                    name: "set-0".into(),
                    val: "2".into(),
                    timestamp: None,
                    tags: Some(tag("production")),
                },
                Metric::Set {
                    name: "set-0".into(),
                    val: "3".into(),
                    timestamp: None,
                    tags: Some(tag("production")),
                },
            ]
        );
    }

    #[test]
    fn metric_buffer_histograms() {
        let sink = BatchSink::new_max(vec![], MetricBuffer::new(), 6, Some(Duration::from_secs(1)));

        let mut events = Vec::new();
        for i in 2..6 {
            let event = Event::Metric(Metric::Histogram {
                name: "hist-2".into(),
                val: 2.0,
                sample_rate: 10,
                timestamp: None,
                tags: Some(tag("production")),
            });
            events.push(event);
        }

        for i in 2..6 {
            let event = Event::Metric(Metric::Histogram {
                name: format!("hist-{}", i),
                val: i as f64,
                sample_rate: 10,
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
        assert_eq!(buffer.len(), 1);

        assert_eq!(
            buffer[0].clone().finish(),
            [
                Metric::Histogram {
                    name: "hist-2".into(),
                    val: 2.0,
                    sample_rate: 50,
                    timestamp: None,
                    tags: Some(tag("production")),
                },
                Metric::Histogram {
                    name: "hist-3".into(),
                    val: 3.0,
                    sample_rate: 10,
                    timestamp: None,
                    tags: Some(tag("production")),
                },
                Metric::Histogram {
                    name: "hist-4".into(),
                    val: 4.0,
                    sample_rate: 10,
                    timestamp: None,
                    tags: Some(tag("production")),
                },
                Metric::Histogram {
                    name: "hist-5".into(),
                    val: 5.0,
                    sample_rate: 10,
                    timestamp: None,
                    tags: Some(tag("production")),
                },
            ]
        );
    }
}
