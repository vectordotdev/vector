use crate::event::metric::{Metric, MetricKind, MetricValue};
use crate::event::Event;
use crate::sinks::util::Batch;
use std::cmp::Ordering;
use std::collections::{hash_map::DefaultHasher, HashSet};
use std::hash::{Hash, Hasher};

#[derive(Clone, Debug)]
pub struct MetricEntry(pub Metric);

impl Eq for MetricEntry {}

impl Hash for MetricEntry {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let metric = &self.0;
        std::mem::discriminant(&metric.value).hash(state);
        metric.name.hash(state);
        metric.kind.hash(state);

        if let Some(tags) = &metric.tags {
            let mut tags: Vec<_> = tags.iter().collect();
            tags.sort();
            for tag in tags {
                tag.hash(state);
            }
        }

        match &metric.value {
            MetricValue::AggregatedHistogram { buckets, .. } => {
                for bucket in buckets {
                    bucket.to_bits().hash(state);
                }
            }
            MetricValue::AggregatedSummary { quantiles, .. } => {
                for quantile in quantiles {
                    quantile.to_bits().hash(state);
                }
            }
            _ => {}
        }
    }
}

impl PartialEq for MetricEntry {
    fn eq(&self, other: &Self) -> bool {
        let mut state = DefaultHasher::new();
        self.hash(&mut state);
        let hash1 = state.finish();

        let mut state = DefaultHasher::new();
        other.hash(&mut state);
        let hash2 = state.finish();

        hash1 == hash2
    }
}

#[derive(Clone, PartialEq)]
pub struct MetricBuffer {
    state: HashSet<MetricEntry>,
    metrics: HashSet<MetricEntry>,
}

impl MetricBuffer {
    // Metric buffer is a data structure for creating normalised
    // batched metrics data from the flow of datapoints.
    //
    // Batching mostly means that we will aggregate away timestamp information, and
    // apply metric-specific compression to improve the performance of the pipeline.
    // For example, multiple counter observations will be summed up into single observation.
    //
    // Normalisation is required to make sure Sources and Sinks are exchanging compatible data
    // structures. For instance, delta gauges produced by Statsd source cannot be directly
    // sent to Datadog API. In this case the buffer will keep the state of a gauge value, and
    // produce absolute values gauges that are well supported by Datadog.
    //
    // Another example of normalisation is disaggregation of counters. Most sinks would expect we send
    // them delta counters (e.g. how many events occured during the flush period). And most sources are
    // producting exactly this kind of counters, with Prometheus being a notable exception. If the counter
    // comes allready aggregated inside the source, the buffer will compare it's values with the previous
    // known and calculate the delta.
    //
    // This table will summarise how metrics are transforming inside the buffer:
    //
    // Normalised and accumulated metrics
    //   Counter                      => Counter
    //   Absolute Counter             => Counter
    //   Gauge                        => Absolute Gauge
    //   Distribution                 => Distribution
    //   Set                          => Set
    //
    // Deduplicated metrics
    //   Absolute Gauge               => Absolute Gauge
    //   AggregatedHistogram          => AggregatedHistogram
    //   AggregatedSummary            => AggregatedSummary
    //   Absolute AggregatedHistogram => Absolute AggregatedHistogram
    //   Absolute AggregatedSummary   => Absolute AggregatedSummary
    //
    pub fn new() -> Self {
        Self {
            state: HashSet::new(),
            metrics: HashSet::new(),
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

        match &item.value {
            MetricValue::Counter { value } if item.kind.is_absolute() => {
                let new = MetricEntry(item.clone());
                if let Some(MetricEntry(Metric {
                    value: MetricValue::Counter { value: value0, .. },
                    ..
                })) = self.state.get(&new)
                {
                    // Counters are disaggregated. We take the previoud value from the state
                    // and emit the difference between previous and current as a Counter
                    let delta = MetricEntry(Metric {
                        name: item.name.to_string(),
                        timestamp: item.timestamp,
                        tags: item.tags.clone(),
                        kind: MetricKind::Incremental,
                        value: MetricValue::Counter {
                            value: value - value0,
                        },
                    });

                    // The resulting Counters could be added up normally
                    if let Some(MetricEntry(mut existing)) = self.metrics.take(&delta) {
                        existing.add(&item);
                        self.metrics.insert(MetricEntry(existing));
                    } else {
                        self.metrics.insert(delta);
                    }
                    self.state.replace(new);
                } else {
                    self.state.insert(new);
                }
            }
            MetricValue::Gauge { .. } if item.kind.is_incremental() => {
                let new = MetricEntry(item.clone().into_absolute());
                if let Some(MetricEntry(mut existing)) = self.metrics.take(&new) {
                    existing.add(&item);
                    self.metrics.insert(MetricEntry(existing));
                } else {
                    // If the metric is not present in active batch,
                    // then we look it up in permanent state, where we keep track
                    // of its values throughout the entire application uptime
                    let mut initial = if let Some(default) = self.state.get(&new) {
                        default.0.clone()
                    } else {
                        // Otherwise we start from zero value
                        Metric {
                            name: item.name.to_string(),
                            timestamp: item.timestamp,
                            tags: item.tags.clone(),
                            kind: MetricKind::Absolute,
                            value: MetricValue::Gauge { value: 0.0 },
                        }
                    };
                    initial.add(&item);
                    self.metrics.insert(MetricEntry(initial));
                }
            }
            _metric if item.kind.is_absolute() => {
                let new = MetricEntry(item);
                self.metrics.replace(new);
            }
            _ => {
                let new = MetricEntry(item.clone());
                if let Some(MetricEntry(mut existing)) = self.metrics.take(&new) {
                    existing.add(&item);
                    self.metrics.insert(MetricEntry(existing));
                } else {
                    self.metrics.insert(new);
                }
            }
        }
    }

    fn is_empty(&self) -> bool {
        self.num_items() == 0
    }

    fn fresh(&self) -> Self {
        let mut state = self.state.clone();
        for entry in self.metrics.iter() {
            if (entry.0.value.is_gauge() || entry.0.value.is_counter())
                && entry.0.kind.is_absolute()
            {
                state.replace(entry.clone());
            }
        }

        Self {
            state,
            metrics: HashSet::new(),
        }
    }

    fn finish(self) -> Self::Output {
        self.metrics
            .into_iter()
            .map(|e| {
                let mut metric = e.0;
                if let MetricValue::Distribution {
                    values,
                    sample_rates,
                } = metric.value
                {
                    let compressed = compress_distribution(values, sample_rates);
                    metric.value = MetricValue::Distribution {
                        values: compressed.0,
                        sample_rates: compressed.1,
                    };
                };
                metric
            })
            .collect()
    }

    fn num_items(&self) -> usize {
        self.metrics.len()
    }
}

fn compress_distribution(values: Vec<f64>, sample_rates: Vec<u32>) -> (Vec<f64>, Vec<u32>) {
    if values.is_empty() || sample_rates.is_empty() {
        return (Vec::new(), Vec::new());
    }

    let mut pairs: Vec<_> = values.into_iter().zip(sample_rates.into_iter()).collect();
    pairs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(Ordering::Equal));

    let mut prev_value = pairs[0].0;
    let mut acc = 0;
    let mut values = vec![];
    let mut sample_rates = vec![];

    for (v, c) in pairs {
        if v == prev_value {
            acc += c;
        } else {
            values.push(prev_value);
            sample_rates.push(acc);
            prev_value = v;
            acc = c;
        }
    }
    values.push(prev_value);
    sample_rates.push(acc);

    (values, sample_rates)
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::sinks::util::{BatchSettings, BatchSink};
    use crate::{
        buffers::Acker,
        event::metric::{Metric, MetricValue},
        runtime::Runtime,
        test_util::runtime,
        Event,
    };
    use futures01::{future, Future, Sink};
    use pretty_assertions::assert_eq;
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use tokio01_test::clock::MockClock;

    fn tag(name: &str) -> BTreeMap<String, String> {
        vec![(name.to_owned(), "true".to_owned())]
            .into_iter()
            .collect()
    }

    fn sorted(buffer: &Vec<Metric>) -> Vec<Metric> {
        let mut buffer = buffer.clone();
        buffer.sort_by_key(|k| format!("{:?}", k));
        buffer
    }

    fn sink() -> (
        impl Sink<SinkItem = Event, SinkError = crate::Error>,
        Runtime,
        MockClock,
        Arc<Mutex<Vec<Vec<Metric>>>>,
    ) {
        let rt = runtime();
        let clock = MockClock::new();

        let (acker, _) = Acker::new_for_testing();
        let sent_requests = Arc::new(Mutex::new(Vec::new()));
        let sent_requests1 = sent_requests.clone();

        let svc = tower::service_fn(move |req| {
            let sent_requests = sent_requests1.clone();

            sent_requests.lock().unwrap().push(req);

            future::ok::<_, std::io::Error>(())
        });
        let buffered = BatchSink::with_executor(
            svc,
            MetricBuffer::new(),
            BatchSettings {
                timeout: Duration::from_secs(0),
                size: 6,
            },
            acker,
            rt.executor(),
        );

        (buffered, rt, clock, sent_requests)
    }

    #[test]
    fn metric_buffer_counters() {
        let (sink, _rt, mut clock, sent_batches) = sink();

        let mut events = Vec::new();
        for i in 0..4 {
            let event = Event::Metric(Metric {
                name: "counter-0".into(),
                timestamp: None,
                tags: Some(tag("production")),
                kind: MetricKind::Incremental,
                value: MetricValue::Counter { value: i as f64 },
            });
            events.push(event);
        }

        for i in 0..4 {
            let event = Event::Metric(Metric {
                name: format!("counter-{}", i),
                timestamp: None,
                tags: Some(tag("staging")),
                kind: MetricKind::Incremental,
                value: MetricValue::Counter { value: i as f64 },
            });
            events.push(event);
        }

        for i in 0..4 {
            let event = Event::Metric(Metric {
                name: format!("counter-{}", i),
                timestamp: None,
                tags: Some(tag("production")),
                kind: MetricKind::Incremental,
                value: MetricValue::Counter { value: i as f64 },
            });
            events.push(event);
        }

        let (sink, _) = clock.enter(|_| {
            sink.sink_map_err(drop)
                .send_all(futures01::stream::iter_ok(events.into_iter()))
                .wait()
                .unwrap()
        });
        drop(sink);

        let buffer = Arc::try_unwrap(sent_batches).unwrap().into_inner().unwrap();

        assert_eq!(buffer.len(), 2);
        assert_eq!(buffer[0].len(), 6);
        assert_eq!(buffer[1].len(), 2);

        assert_eq!(
            sorted(&buffer[0].clone().finish()),
            [
                Metric {
                    name: "counter-0".into(),
                    timestamp: None,
                    tags: Some(tag("production")),
                    kind: MetricKind::Incremental,
                    value: MetricValue::Counter { value: 6.0 }
                },
                Metric {
                    name: "counter-0".into(),
                    timestamp: None,
                    tags: Some(tag("staging")),
                    kind: MetricKind::Incremental,
                    value: MetricValue::Counter { value: 0.0 },
                },
                Metric {
                    name: "counter-1".into(),
                    timestamp: None,
                    tags: Some(tag("production")),
                    kind: MetricKind::Incremental,
                    value: MetricValue::Counter { value: 1.0 },
                },
                Metric {
                    name: "counter-1".into(),
                    timestamp: None,
                    tags: Some(tag("staging")),
                    kind: MetricKind::Incremental,
                    value: MetricValue::Counter { value: 1.0 },
                },
                Metric {
                    name: "counter-2".into(),
                    timestamp: None,
                    tags: Some(tag("staging")),
                    kind: MetricKind::Incremental,
                    value: MetricValue::Counter { value: 2.0 },
                },
                Metric {
                    name: "counter-3".into(),
                    timestamp: None,
                    tags: Some(tag("staging")),
                    kind: MetricKind::Incremental,
                    value: MetricValue::Counter { value: 3.0 },
                },
            ]
        );

        assert_eq!(
            sorted(&buffer[1].clone().finish()),
            [
                Metric {
                    name: "counter-2".into(),
                    timestamp: None,
                    tags: Some(tag("production")),
                    kind: MetricKind::Incremental,
                    value: MetricValue::Counter { value: 2.0 },
                },
                Metric {
                    name: "counter-3".into(),
                    timestamp: None,
                    tags: Some(tag("production")),
                    kind: MetricKind::Incremental,
                    value: MetricValue::Counter { value: 3.0 },
                },
            ]
        );
    }

    #[test]
    fn metric_buffer_aggregated_counters() {
        let (sink, _rt, mut clock, sent_batches) = sink();

        let mut events = Vec::new();
        for i in 0..4 {
            let event = Event::Metric(Metric {
                name: format!("counter-{}", i),
                timestamp: None,
                tags: Some(tag("production")),
                kind: MetricKind::Absolute,
                value: MetricValue::Counter { value: i as f64 },
            });
            events.push(event);
        }

        for i in 0..4 {
            let event = Event::Metric(Metric {
                name: format!("counter-{}", i),
                timestamp: None,
                tags: Some(tag("production")),
                kind: MetricKind::Absolute,
                value: MetricValue::Counter {
                    value: i as f64 * 3.0,
                },
            });
            events.push(event);
        }

        let (sink, _) = clock.enter(|_| {
            sink.sink_map_err(drop)
                .send_all(futures01::stream::iter_ok(events.into_iter()))
                .wait()
                .unwrap()
        });
        drop(sink);

        let buffer = Arc::try_unwrap(sent_batches).unwrap().into_inner().unwrap();

        assert_eq!(buffer.len(), 1);
        assert_eq!(buffer[0].len(), 4);

        assert_eq!(
            sorted(&buffer[0].clone().finish()),
            [
                Metric {
                    name: "counter-0".into(),
                    timestamp: None,
                    tags: Some(tag("production")),
                    kind: MetricKind::Incremental,
                    value: MetricValue::Counter { value: 0.0 },
                },
                Metric {
                    name: "counter-1".into(),
                    timestamp: None,
                    tags: Some(tag("production")),
                    kind: MetricKind::Incremental,
                    value: MetricValue::Counter { value: 2.0 },
                },
                Metric {
                    name: "counter-2".into(),
                    timestamp: None,
                    tags: Some(tag("production")),
                    kind: MetricKind::Incremental,
                    value: MetricValue::Counter { value: 4.0 },
                },
                Metric {
                    name: "counter-3".into(),
                    timestamp: None,
                    tags: Some(tag("production")),
                    kind: MetricKind::Incremental,
                    value: MetricValue::Counter { value: 6.0 },
                },
            ]
        );
    }

    #[test]
    fn metric_buffer_gauges() {
        let (sink, _rt, mut clock, sent_batches) = sink();

        let mut events = Vec::new();
        for i in 1..5 {
            let event = Event::Metric(Metric {
                name: format!("gauge-{}", i),
                timestamp: None,
                tags: Some(tag("staging")),
                kind: MetricKind::Incremental,
                value: MetricValue::Gauge { value: i as f64 },
            });
            events.push(event);
        }

        for i in 1..5 {
            let event = Event::Metric(Metric {
                name: format!("gauge-{}", i),
                timestamp: None,
                tags: Some(tag("staging")),
                kind: MetricKind::Incremental,
                value: MetricValue::Gauge { value: i as f64 },
            });
            events.push(event);
        }

        let (sink, _) = clock.enter(|_| {
            sink.sink_map_err(drop)
                .send_all(futures01::stream::iter_ok(events.into_iter()))
                .wait()
                .unwrap()
        });
        drop(sink);

        let buffer = Arc::try_unwrap(sent_batches).unwrap().into_inner().unwrap();

        assert_eq!(buffer.len(), 1);
        assert_eq!(buffer[0].len(), 4);

        assert_eq!(
            sorted(&buffer[0].clone().finish()),
            [
                Metric {
                    name: "gauge-1".into(),
                    timestamp: None,
                    tags: Some(tag("staging")),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge { value: 2.0 },
                },
                Metric {
                    name: "gauge-2".into(),
                    timestamp: None,
                    tags: Some(tag("staging")),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge { value: 4.0 },
                },
                Metric {
                    name: "gauge-3".into(),
                    timestamp: None,
                    tags: Some(tag("staging")),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge { value: 6.0 },
                },
                Metric {
                    name: "gauge-4".into(),
                    timestamp: None,
                    tags: Some(tag("staging")),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge { value: 8.0 },
                },
            ]
        );
    }

    #[test]
    fn metric_buffer_aggregated_gauges() {
        let (sink, _rt, mut clock, sent_batches) = sink();

        let mut events = Vec::new();
        for i in 3..6 {
            let event = Event::Metric(Metric {
                name: format!("gauge-{}", i),
                timestamp: None,
                tags: Some(tag("staging")),
                kind: MetricKind::Absolute,
                value: MetricValue::Gauge {
                    value: i as f64 * 10.0,
                },
            });
            events.push(event);
        }

        for i in 1..4 {
            let event = Event::Metric(Metric {
                name: format!("gauge-{}", i),
                timestamp: None,
                tags: Some(tag("staging")),
                kind: MetricKind::Incremental,
                value: MetricValue::Gauge { value: i as f64 },
            });
            events.push(event);
        }

        for i in 2..5 {
            let event = Event::Metric(Metric {
                name: format!("gauge-{}", i),
                timestamp: None,
                tags: Some(tag("staging")),
                kind: MetricKind::Absolute,
                value: MetricValue::Gauge {
                    value: i as f64 * 2.0,
                },
            });
            events.push(event);
        }

        let (sink, _) = clock.enter(|_| {
            sink.sink_map_err(drop)
                .send_all(futures01::stream::iter_ok(events.into_iter()))
                .wait()
                .unwrap()
        });
        drop(sink);

        let buffer = Arc::try_unwrap(sent_batches).unwrap().into_inner().unwrap();

        assert_eq!(buffer.len(), 1);
        assert_eq!(buffer[0].len(), 5);

        assert_eq!(
            sorted(&buffer[0].clone().finish()),
            [
                Metric {
                    name: "gauge-1".into(),
                    timestamp: None,
                    tags: Some(tag("staging")),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge { value: 1.0 },
                },
                Metric {
                    name: "gauge-2".into(),
                    timestamp: None,
                    tags: Some(tag("staging")),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge { value: 4.0 },
                },
                Metric {
                    name: "gauge-3".into(),
                    timestamp: None,
                    tags: Some(tag("staging")),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge { value: 6.0 },
                },
                Metric {
                    name: "gauge-4".into(),
                    timestamp: None,
                    tags: Some(tag("staging")),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge { value: 8.0 },
                },
                Metric {
                    name: "gauge-5".into(),
                    timestamp: None,
                    tags: Some(tag("staging")),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge { value: 50.0 },
                },
            ]
        );
    }

    #[test]
    fn metric_buffer_sets() {
        let (sink, _rt, mut clock, sent_batches) = sink();

        let mut events = Vec::new();
        for i in 0..4 {
            let event = Event::Metric(Metric {
                name: "set-0".into(),
                timestamp: None,
                tags: Some(tag("production")),
                kind: MetricKind::Incremental,
                value: MetricValue::Set {
                    values: vec![format!("{}", i)].into_iter().collect(),
                },
            });
            events.push(event);
        }

        for i in 0..4 {
            let event = Event::Metric(Metric {
                name: "set-0".into(),
                timestamp: None,
                tags: Some(tag("production")),
                kind: MetricKind::Incremental,
                value: MetricValue::Set {
                    values: vec![format!("{}", i)].into_iter().collect(),
                },
            });
            events.push(event);
        }

        let (sink, _) = clock.enter(|_| {
            sink.sink_map_err(drop)
                .send_all(futures01::stream::iter_ok(events.into_iter()))
                .wait()
                .unwrap()
        });
        drop(sink);

        let buffer = Arc::try_unwrap(sent_batches).unwrap().into_inner().unwrap();

        assert_eq!(buffer.len(), 1);

        assert_eq!(
            sorted(&buffer[0].clone().finish()),
            [Metric {
                name: "set-0".into(),
                timestamp: None,
                tags: Some(tag("production")),
                kind: MetricKind::Incremental,
                value: MetricValue::Set {
                    values: vec!["0".into(), "1".into(), "2".into(), "3".into()]
                        .into_iter()
                        .collect(),
                },
            },]
        );
    }

    #[test]
    fn metric_buffer_distributions() {
        let (sink, _rt, mut clock, sent_batches) = sink();

        let mut events = Vec::new();
        for _ in 2..6 {
            let event = Event::Metric(Metric {
                name: "dist-2".into(),
                timestamp: None,
                tags: Some(tag("production")),
                kind: MetricKind::Incremental,
                value: MetricValue::Distribution {
                    values: vec![2.0],
                    sample_rates: vec![10],
                },
            });
            events.push(event);
        }

        for i in 2..6 {
            let event = Event::Metric(Metric {
                name: format!("dist-{}", i),
                timestamp: None,
                tags: Some(tag("production")),
                kind: MetricKind::Incremental,
                value: MetricValue::Distribution {
                    values: vec![i as f64],
                    sample_rates: vec![10],
                },
            });
            events.push(event);
        }

        let (sink, _) = clock.enter(|_| {
            sink.sink_map_err(drop)
                .send_all(futures01::stream::iter_ok(events.into_iter()))
                .wait()
                .unwrap()
        });
        drop(sink);

        let buffer = Arc::try_unwrap(sent_batches).unwrap().into_inner().unwrap();

        assert_eq!(buffer.len(), 1);

        assert_eq!(
            sorted(&buffer[0].clone().finish()),
            [
                Metric {
                    name: "dist-2".into(),
                    timestamp: None,
                    tags: Some(tag("production")),
                    kind: MetricKind::Incremental,
                    value: MetricValue::Distribution {
                        values: vec![2.0],
                        sample_rates: vec![50],
                    },
                },
                Metric {
                    name: "dist-3".into(),
                    timestamp: None,
                    tags: Some(tag("production")),
                    kind: MetricKind::Incremental,
                    value: MetricValue::Distribution {
                        values: vec![3.0],
                        sample_rates: vec![10],
                    },
                },
                Metric {
                    name: "dist-4".into(),
                    timestamp: None,
                    tags: Some(tag("production")),
                    kind: MetricKind::Incremental,
                    value: MetricValue::Distribution {
                        values: vec![4.0],
                        sample_rates: vec![10],
                    },
                },
                Metric {
                    name: "dist-5".into(),
                    timestamp: None,
                    tags: Some(tag("production")),
                    kind: MetricKind::Incremental,
                    value: MetricValue::Distribution {
                        values: vec![5.0],
                        sample_rates: vec![10],
                    }
                },
            ]
        );
    }

    #[test]
    fn metric_buffer_compress_distribution() {
        let values = vec![2.0, 2.0, 3.0, 1.0, 2.0, 2.0, 3.0];
        let sample_rates = vec![12, 12, 13, 11, 12, 12, 13];

        assert_eq!(
            compress_distribution(values, sample_rates),
            (vec![1.0, 2.0, 3.0], vec![11, 48, 26])
        );
    }

    #[test]
    fn metric_buffer_aggregated_histograms_absolute() {
        let (sink, _rt, mut clock, sent_batches) = sink();

        let mut events = Vec::new();
        for _ in 2..5 {
            let event = Event::Metric(Metric {
                name: "buckets-2".into(),
                timestamp: None,
                tags: Some(tag("production")),
                kind: MetricKind::Absolute,
                value: MetricValue::AggregatedHistogram {
                    buckets: vec![1.0, 2.0, 4.0],
                    counts: vec![1, 2, 4],
                    count: 6,
                    sum: 10.0,
                },
            });
            events.push(event);
        }

        for i in 2..5 {
            let event = Event::Metric(Metric {
                name: format!("buckets-{}", i),
                timestamp: None,
                tags: Some(tag("production")),
                kind: MetricKind::Absolute,
                value: MetricValue::AggregatedHistogram {
                    buckets: vec![1.0, 2.0, 4.0],
                    counts: vec![1 * i, 2 * i, 4 * i],
                    count: 6 * i,
                    sum: 10.0,
                },
            });
            events.push(event);
        }

        let (sink, _) = clock.enter(|_| {
            sink.sink_map_err(drop)
                .send_all(futures01::stream::iter_ok(events.into_iter()))
                .wait()
                .unwrap()
        });
        drop(sink);

        let buffer = Arc::try_unwrap(sent_batches).unwrap().into_inner().unwrap();

        assert_eq!(buffer.len(), 1);

        assert_eq!(
            sorted(&buffer[0].clone().finish()),
            [
                Metric {
                    name: "buckets-2".into(),
                    timestamp: None,
                    tags: Some(tag("production")),
                    kind: MetricKind::Absolute,
                    value: MetricValue::AggregatedHistogram {
                        buckets: vec![1.0, 2.0, 4.0],
                        counts: vec![2, 4, 8],
                        count: 12,
                        sum: 10.0,
                    },
                },
                Metric {
                    name: "buckets-3".into(),
                    timestamp: None,
                    tags: Some(tag("production")),
                    kind: MetricKind::Absolute,
                    value: MetricValue::AggregatedHistogram {
                        buckets: vec![1.0, 2.0, 4.0],
                        counts: vec![3, 6, 12],
                        count: 6 * 3,
                        sum: 10.0,
                    },
                },
                Metric {
                    name: "buckets-4".into(),
                    timestamp: None,
                    tags: Some(tag("production")),
                    kind: MetricKind::Absolute,
                    value: MetricValue::AggregatedHistogram {
                        buckets: vec![1.0, 2.0, 4.0],
                        counts: vec![4, 8, 16],
                        count: 6 * 4,
                        sum: 10.0,
                    },
                }
            ]
        );
    }

    #[test]
    fn metric_buffer_aggregated_histograms_incremental() {
        let (sink, _rt, mut clock, sent_batches) = sink();

        let mut events = Vec::new();
        for _ in 0..3 {
            let event = Event::Metric(Metric {
                name: "buckets-2".into(),
                timestamp: None,
                tags: Some(tag("production")),
                kind: MetricKind::Incremental,
                value: MetricValue::AggregatedHistogram {
                    buckets: vec![1.0, 2.0, 4.0],
                    counts: vec![1, 2, 4],
                    count: 6,
                    sum: 10.0,
                },
            });
            events.push(event);
        }

        for i in 1..4 {
            let event = Event::Metric(Metric {
                name: "buckets-2".into(),
                timestamp: None,
                tags: Some(tag("production")),
                kind: MetricKind::Incremental,
                value: MetricValue::AggregatedHistogram {
                    buckets: vec![1.0, 4.0, 16.0],
                    counts: vec![1 * i, 2 * i, 4 * i],
                    count: 6 * i,
                    sum: 10.0,
                },
            });
            events.push(event);
        }

        let (sink, _) = clock.enter(|_| {
            sink.sink_map_err(drop)
                .send_all(futures01::stream::iter_ok(events.into_iter()))
                .wait()
                .unwrap()
        });
        drop(sink);

        let buffer = Arc::try_unwrap(sent_batches).unwrap().into_inner().unwrap();

        assert_eq!(buffer.len(), 1);

        assert_eq!(
            sorted(&buffer[0].clone().finish()),
            [
                Metric {
                    name: "buckets-2".into(),
                    timestamp: None,
                    tags: Some(tag("production")),
                    kind: MetricKind::Incremental,
                    value: MetricValue::AggregatedHistogram {
                        buckets: vec![1.0, 2.0, 4.0],
                        counts: vec![3, 6, 12],
                        count: 18,
                        sum: 30.0,
                    },
                },
                Metric {
                    name: "buckets-2".into(),
                    timestamp: None,
                    tags: Some(tag("production")),
                    kind: MetricKind::Incremental,
                    value: MetricValue::AggregatedHistogram {
                        buckets: vec![1.0, 4.0, 16.0],
                        counts: vec![6, 12, 24],
                        count: 36,
                        sum: 30.0,
                    },
                },
            ]
        );
    }

    #[test]
    fn metric_buffer_aggregated_summaries() {
        let (sink, _rt, mut clock, sent_batches) = sink();

        let mut events = Vec::new();
        for _ in 0..10 {
            for i in 2..5 {
                let event = Event::Metric(Metric {
                    name: format!("quantiles-{}", i),
                    timestamp: None,
                    tags: Some(tag("production")),
                    kind: MetricKind::Absolute,
                    value: MetricValue::AggregatedSummary {
                        quantiles: vec![0.0, 0.5, 1.0],
                        values: vec![(1 * i) as f64, (2 * i) as f64, (4 * i) as f64],
                        count: 6 * i,
                        sum: 10.0,
                    },
                });
                events.push(event);
            }
        }

        let (sink, _) = clock.enter(|_| {
            sink.sink_map_err(drop)
                .send_all(futures01::stream::iter_ok(events.into_iter()))
                .wait()
                .unwrap()
        });
        drop(sink);

        let buffer = Arc::try_unwrap(sent_batches).unwrap().into_inner().unwrap();

        assert_eq!(buffer.len(), 1);

        assert_eq!(
            sorted(&buffer[0].clone().finish()),
            [
                Metric {
                    name: "quantiles-2".into(),
                    timestamp: None,
                    tags: Some(tag("production")),
                    kind: MetricKind::Absolute,
                    value: MetricValue::AggregatedSummary {
                        quantiles: vec![0.0, 0.5, 1.0],
                        values: vec![2.0, 4.0, 8.0],
                        count: 6 * 2,
                        sum: 10.0,
                    },
                },
                Metric {
                    name: "quantiles-3".into(),
                    timestamp: None,
                    tags: Some(tag("production")),
                    kind: MetricKind::Absolute,
                    value: MetricValue::AggregatedSummary {
                        quantiles: vec![0.0, 0.5, 1.0],
                        values: vec![3.0, 6.0, 12.0],
                        count: 6 * 3,
                        sum: 10.0,
                    },
                },
                Metric {
                    name: "quantiles-4".into(),
                    timestamp: None,
                    tags: Some(tag("production")),
                    kind: MetricKind::Absolute,
                    value: MetricValue::AggregatedSummary {
                        quantiles: vec![0.0, 0.5, 1.0],
                        values: vec![4.0, 8.0, 16.0],
                        count: 6 * 4,
                        sum: 10.0,
                    },
                }
            ]
        );
    }
}
