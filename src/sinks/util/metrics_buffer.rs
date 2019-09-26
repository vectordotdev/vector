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
    gauges: IndexMap<MetricKey, Metric>,
    metrics: IndexMap<MetricKey, Metric>,
}

impl MetricBuffer {
    pub fn new() -> Self {
        Self {
            gauges: IndexMap::new(),
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

        if item.is_mergeable() {
            let key = MetricKey::new(item.name(), item.tags());
            if let Some(metric) = self.metrics.get_mut(&key) {
                metric.merge(&item);
            } else {
                self.metrics.insert(key, item);
            }
        }
    }

    fn is_empty(&self) -> bool {
        self.num_items() == 0
    }

    fn fresh(&self) -> Self {
        Self {
            gauges: self.gauges.clone(),
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
}
