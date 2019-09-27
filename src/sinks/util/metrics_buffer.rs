use super::batch::Batch;
use crate::event::{Event, Metric};
use indexmap::IndexMap;
use std::collections::HashMap;

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct MetricKey {
    name: String,
    tags: Option<Vec<(String, String)>>,
}

impl MetricKey {
    fn new(name: &str, tags: &Option<HashMap<String, String>>) -> Self {
        Self {
            name: name.to_owned(),
            tags: tags.clone().map(|m| m.into_iter().collect()),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MetricBuffer {
    state: IndexMap<MetricKey, Metric>,
    metrics: IndexMap<MetricKey, Metric>,
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

        match item {
            Metric::Counter {
                ref name, ref tags, ..
            } => {
                let key = MetricKey::new(name, tags);
                if let Some(metric) = self.metrics.get_mut(&key) {
                    metric.merge(&item);
                } else {
                    self.metrics.insert(key, item);
                }
            }
            Metric::Gauge {
                ref name,
                ref val,
                ref tags,
                ..
            } => {
                let key = MetricKey::new(name, tags);
                if let Some(metric) = self.metrics.get_mut(&key) {
                    metric.merge(&item);
                } else {
                    let default = Metric::Gauge {
                        name: name.clone(),
                        val: *val,
                        direction: None,
                        timestamp: None,
                        tags: tags.clone(),
                    };
                    self.metrics.insert(key, default);
                }
            }
            _ => {}
        }
    }

    fn is_empty(&self) -> bool {
        self.num_items() == 0
    }

    fn fresh(&self) -> Self {
        let mut state = self.state.clone();
        for (k, v) in self.metrics.iter() {
            match v {
                Metric::Gauge { .. } => {
                    state.insert(k.clone(), v.clone());
                }
                _ => {}
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
        let sink = BatchSink::new_min_max(
            vec![],
            MetricBuffer::new(),
            4,
            0,
            Some(Duration::from_secs(1)),
        );

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

        for i in 0..4 {
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
        assert_eq!(buffer[2].len(), 2);

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
            buffer[2].clone().finish(),
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
            ]
        );
    }
}
