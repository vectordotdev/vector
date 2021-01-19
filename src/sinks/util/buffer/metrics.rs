use crate::{
    event::metric::{Metric, MetricData, MetricKind, MetricValue, Sample},
    sinks::util::batch::{Batch, BatchConfig, BatchError, BatchSettings, BatchSize, PushResult},
    Event,
};
use std::{
    cmp::Ordering,
    collections::HashSet,
    hash::{Hash, Hasher},
    mem::discriminant,
    ops::Deref,
};

#[derive(Clone, Debug)]
pub struct MetricEntry(pub Metric);

impl Eq for MetricEntry {}

impl Hash for MetricEntry {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let metric = &self.0;
        metric.series.hash(state);
        metric.data.kind.hash(state);
        discriminant(&metric.data.value).hash(state);

        match &metric.data.value {
            MetricValue::AggregatedHistogram { buckets, .. } => {
                for bucket in buckets {
                    bucket.upper_limit.to_bits().hash(state);
                }
            }
            MetricValue::AggregatedSummary { quantiles, .. } => {
                for quantile in quantiles {
                    quantile.upper_limit.to_bits().hash(state);
                }
            }
            _ => {}
        }
    }
}

impl PartialEq for MetricEntry {
    fn eq(&self, other: &Self) -> bool {
        // This differs from a straightforward implementation of `eq` by
        // comparing only the "shape" bits (name, tags, and type) while
        // allowing the contained values to be different.
        self.series == other.series
            && self.data.kind == other.data.kind
            && discriminant(&self.data.value) == discriminant(&other.data.value)
            && match (&self.data.value, &other.data.value) {
                (
                    MetricValue::AggregatedHistogram {
                        buckets: buckets1, ..
                    },
                    MetricValue::AggregatedHistogram {
                        buckets: buckets2, ..
                    },
                ) => {
                    buckets1.len() == buckets2.len()
                        && buckets1
                            .iter()
                            .zip(buckets2.iter())
                            .all(|(b1, b2)| b1.upper_limit == b2.upper_limit)
                }
                (
                    MetricValue::AggregatedSummary {
                        quantiles: quantiles1,
                        ..
                    },
                    MetricValue::AggregatedSummary {
                        quantiles: quantiles2,
                        ..
                    },
                ) => {
                    quantiles1.len() == quantiles2.len()
                        && quantiles1
                            .iter()
                            .zip(quantiles2.iter())
                            .all(|(q1, q2)| q1.upper_limit == q2.upper_limit)
                }
                _ => true,
            }
    }
}

impl Deref for MetricEntry {
    type Target = Metric;
    fn deref(&self) -> &Metric {
        &self.0
    }
}

#[derive(Clone, PartialEq)]
pub struct MetricBuffer {
    state: HashSet<MetricEntry>,
    metrics: HashSet<MetricEntry>,
    max_events: usize,
}

impl MetricBuffer {
    // Metric buffer is a data structure for creating normalised
    // batched metrics data from the flow of data points.
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
    // them delta counters (e.g. how many events occurred during the flush period). And most sources are
    // producing exactly these kind of counters, with Prometheus being a notable exception. If the counter
    // comes already aggregated inside the source, the buffer will compare it's values with the previous
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
    pub fn new(settings: BatchSize<Self>) -> Self {
        Self::new_with_state(settings.events, HashSet::new())
    }

    fn new_with_state(max_events: usize, state: HashSet<MetricEntry>) -> Self {
        Self {
            state,
            metrics: HashSet::with_capacity(max_events),
            max_events,
        }
    }
}

impl Batch for MetricBuffer {
    type Input = Event;
    type Output = Vec<Metric>;

    fn get_settings_defaults(
        config: BatchConfig,
        defaults: BatchSettings<Self>,
    ) -> Result<BatchSettings<Self>, BatchError> {
        Ok(config
            .disallow_max_bytes()?
            .use_size_as_events()?
            .get_settings_or_default(defaults))
    }

    fn push(&mut self, item: Self::Input) -> PushResult<Self::Input> {
        if self.num_items() >= self.max_events {
            PushResult::Overflow(item)
        } else {
            let item = item.into_metric();

            match (item.data.kind, &item.data.value) {
                (MetricKind::Absolute, MetricValue::Counter { value }) => {
                    let value = *value;
                    let item = MetricEntry(item);
                    if let Some(MetricEntry(Metric {
                        data:
                            MetricData {
                                value: MetricValue::Counter { value: value0, .. },
                                ..
                            },
                        ..
                    })) = self.state.get(&item)
                    {
                        // Counters are disaggregated. We take the previous value from the state
                        // and emit the difference between previous and current as a Counter
                        let delta = MetricEntry(Metric {
                            series: item.series.clone(),
                            data: MetricData {
                                timestamp: item.data.timestamp,
                                kind: MetricKind::Incremental,
                                value: MetricValue::Counter {
                                    value: value - value0,
                                },
                            },
                        });

                        // The resulting Counters could be added up normally
                        if let Some(MetricEntry(mut existing)) = self.metrics.take(&delta) {
                            existing.data.add(&item.data);
                            self.metrics.insert(MetricEntry(existing));
                        } else {
                            self.metrics.insert(delta);
                        }
                        self.state.replace(item);
                    } else {
                        self.state.insert(item);
                    }
                }
                (MetricKind::Incremental, MetricValue::Gauge { .. }) => {
                    let new = MetricEntry(item.to_absolute());
                    if let Some(MetricEntry(mut existing)) = self.metrics.take(&new) {
                        existing.data.add(&item.data);
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
                                series: item.series.clone(),
                                data: MetricData {
                                    timestamp: item.data.timestamp,
                                    kind: MetricKind::Absolute,
                                    value: MetricValue::Gauge { value: 0.0 },
                                },
                            }
                        };
                        initial.data.add(&item.data);
                        self.metrics.insert(MetricEntry(initial));
                    }
                }
                (MetricKind::Absolute, _) => {
                    self.metrics.replace(MetricEntry(item));
                }
                _ => {
                    let new = MetricEntry(item);
                    if let Some(MetricEntry(mut existing)) = self.metrics.take(&new) {
                        existing.data.add(&new.data);
                        self.metrics.insert(MetricEntry(existing));
                    } else {
                        self.metrics.insert(new);
                    }
                }
            }
            PushResult::Ok(self.num_items() >= self.max_events)
        }
    }

    fn is_empty(&self) -> bool {
        self.num_items() == 0
    }

    fn fresh(&self) -> Self {
        let mut state = self.state.clone();
        for entry in self.metrics.iter() {
            let data = &entry.0.data;
            if (data.value.is_gauge() || data.value.is_counter()) && data.kind.is_absolute() {
                state.replace(entry.clone());
            }
        }

        Self::new_with_state(self.max_events, state)
    }

    fn finish(self) -> Self::Output {
        self.metrics
            .into_iter()
            .map(|e| {
                let mut metric = e.0;
                if let MetricValue::Distribution { samples, statistic } = metric.data.value {
                    let samples = compress_distribution(samples);
                    metric.data.value = MetricValue::Distribution { samples, statistic };
                };
                metric
            })
            .collect()
    }

    fn num_items(&self) -> usize {
        self.metrics.len()
    }
}

fn compress_distribution(mut samples: Vec<Sample>) -> Vec<Sample> {
    if samples.is_empty() {
        return Vec::new();
    }

    samples.sort_by(|a, b| a.value.partial_cmp(&b.value).unwrap_or(Ordering::Equal));

    let mut acc = Sample {
        value: samples[0].value,
        rate: 0,
    };
    let mut result = Vec::new();

    for sample in samples {
        if acc.value == sample.value {
            acc.rate += sample.rate;
        } else {
            result.push(acc);
            acc = sample;
        }
    }
    result.push(acc);

    result
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::event::metric::{MetricKind::*, MetricValue, StatisticKind};
    use pretty_assertions::assert_eq;
    use std::collections::BTreeMap;

    fn tag(name: &str) -> BTreeMap<String, String> {
        vec![(name.to_owned(), "true".to_owned())]
            .into_iter()
            .collect()
    }

    fn rebuffer(events: Vec<Metric>) -> Vec<Vec<Metric>> {
        let batch_size = BatchSettings::default().bytes(9999).events(6).size;
        let mut buffer = MetricBuffer::new(batch_size);
        let mut result = vec![];

        for event in events {
            match buffer.push(Event::Metric(event)) {
                PushResult::Overflow(_) => panic!("overflowed too early"),
                PushResult::Ok(true) => {
                    result.push(buffer.fresh_replace().finish());
                }
                PushResult::Ok(false) => (),
            }
        }

        if !buffer.is_empty() {
            result.push(buffer.finish())
        }

        // Sort each batch to provide a predictable result ordering
        result
            .into_iter()
            .map(|mut batch| {
                batch.sort_by_key(|k| format!("{:?}", k));
                batch
            })
            .collect()
    }

    #[test]
    fn metric_buffer_counters() {
        let mut events = Vec::new();
        for i in 0..4 {
            events.push(sample_counter(0, "production", Incremental, i as f64));
        }

        for i in 0..4 {
            events.push(sample_counter(i, "staging", Incremental, i as f64));
        }

        for i in 0..4 {
            events.push(sample_counter(i, "production", Incremental, i as f64));
        }

        let buffer = rebuffer(events);

        assert_eq!(
            buffer[0],
            [
                sample_counter(0, "production", Incremental, 6.0),
                sample_counter(0, "staging", Incremental, 0.0),
                sample_counter(1, "production", Incremental, 1.0),
                sample_counter(1, "staging", Incremental, 1.0),
                sample_counter(2, "staging", Incremental, 2.0),
                sample_counter(3, "staging", Incremental, 3.0),
            ]
        );

        assert_eq!(
            buffer[1],
            [
                sample_counter(2, "production", Incremental, 2.0),
                sample_counter(3, "production", Incremental, 3.0),
            ]
        );

        assert_eq!(buffer.len(), 2);
    }

    #[test]
    fn metric_buffer_aggregated_counters() {
        let mut events = Vec::new();
        for i in 0..4 {
            events.push(sample_counter(i, "production", Absolute, i as f64));
        }

        for i in 0..4 {
            events.push(sample_counter(i, "production", Absolute, i as f64 * 3.0));
        }

        let buffer = rebuffer(events);

        assert_eq!(
            buffer[0],
            [
                sample_counter(0, "production", Incremental, 0.0),
                sample_counter(1, "production", Incremental, 2.0),
                sample_counter(2, "production", Incremental, 4.0),
                sample_counter(3, "production", Incremental, 6.0),
            ]
        );

        assert_eq!(buffer.len(), 1);
    }

    #[test]
    fn metric_buffer_gauges() {
        let mut events = Vec::new();
        for i in 1..5 {
            events.push(sample_gauge(i, Incremental, i as f64));
        }

        for i in 1..5 {
            events.push(sample_gauge(i, Incremental, i as f64));
        }

        let buffer = rebuffer(events);

        assert_eq!(
            buffer[0],
            [
                sample_gauge(1, Absolute, 2.0),
                sample_gauge(2, Absolute, 4.0),
                sample_gauge(3, Absolute, 6.0),
                sample_gauge(4, Absolute, 8.0),
            ]
        );

        assert_eq!(buffer.len(), 1);
    }

    #[test]
    fn metric_buffer_aggregated_gauges() {
        let mut events = Vec::new();
        for i in 3..6 {
            events.push(sample_gauge(i, Absolute, i as f64 * 10.0));
        }

        for i in 1..4 {
            events.push(sample_gauge(i, Incremental, i as f64));
        }

        for i in 2..5 {
            events.push(sample_gauge(i, Absolute, i as f64 * 2.0));
        }

        let buffer = rebuffer(events);

        assert_eq!(
            buffer[0],
            [
                sample_gauge(1, Absolute, 1.0),
                sample_gauge(2, Absolute, 4.0),
                sample_gauge(3, Absolute, 6.0),
                sample_gauge(4, Absolute, 8.0),
                sample_gauge(5, Absolute, 50.0),
            ]
        );

        assert_eq!(buffer.len(), 1);
    }

    #[test]
    fn metric_buffer_sets() {
        let mut events = Vec::new();
        for i in 0..4 {
            events.push(sample_set(0, &[i]));
        }

        for i in 0..4 {
            events.push(sample_set(0, &[i]));
        }

        let buffer = rebuffer(events);

        assert_eq!(buffer[0], [sample_set(0, &[0, 1, 2, 3])]);

        assert_eq!(buffer.len(), 1);
    }

    #[test]
    fn metric_buffer_distributions() {
        let mut events = Vec::new();
        for _ in 2..6 {
            events.push(sample_distribution_histogram(2, 10));
        }

        for i in 2..6 {
            events.push(sample_distribution_histogram(i, 10));
        }

        let buffer = rebuffer(events);

        assert_eq!(
            buffer[0],
            [
                sample_distribution_histogram(2, 50),
                sample_distribution_histogram(3, 10),
                sample_distribution_histogram(4, 10),
                sample_distribution_histogram(5, 10),
            ]
        );

        assert_eq!(buffer.len(), 1);
    }

    #[test]
    fn metric_buffer_compress_distribution() {
        let samples = crate::samples![
            2.0 => 12,
            2.0 => 12,
            3.0 => 13,
            1.0 => 11,
            2.0 => 12,
            2.0 => 12,
            3.0 => 13
        ];

        assert_eq!(
            compress_distribution(samples),
            crate::samples![1.0 => 11, 2.0 => 48, 3.0 => 26]
        );
    }

    #[test]
    fn metric_buffer_aggregated_histograms_absolute() {
        let mut events = Vec::new();
        for _ in 2..5 {
            events.push(sample_aggregated_histogram(2, Absolute, 1.0, 1, 10.0));
        }

        for i in 2..5 {
            events.push(sample_aggregated_histogram(
                i, Absolute, 1.0, i as u32, 10.0,
            ));
        }

        let buffer = rebuffer(events);

        assert_eq!(
            buffer[0],
            [
                sample_aggregated_histogram(2, Absolute, 1.0, 2, 10.0),
                sample_aggregated_histogram(3, Absolute, 1.0, 3, 10.0),
                sample_aggregated_histogram(4, Absolute, 1.0, 4, 10.0),
            ]
        );

        assert_eq!(buffer.len(), 1);
    }

    #[test]
    fn metric_buffer_aggregated_histograms_incremental() {
        let mut events = Vec::new();
        for _ in 0..3 {
            events.push(sample_aggregated_histogram(2, Incremental, 1.0, 1, 10.0));
        }

        for i in 1..4 {
            events.push(sample_aggregated_histogram(2, Incremental, 2.0, i, 10.0));
        }

        let buffer = rebuffer(events);

        assert_eq!(
            buffer[0],
            [
                sample_aggregated_histogram(2, Incremental, 1.0, 3, 30.0),
                sample_aggregated_histogram(2, Incremental, 2.0, 6, 30.0),
            ]
        );

        assert_eq!(buffer.len(), 1);
    }

    #[test]
    fn metric_buffer_aggregated_summaries() {
        let mut events = Vec::new();
        for _ in 0..10 {
            for i in 2..5 {
                events.push(sample_aggregated_summary(i));
            }
        }

        let buffer = rebuffer(events);

        assert_eq!(
            buffer[0],
            [
                sample_aggregated_summary(2),
                sample_aggregated_summary(3),
                sample_aggregated_summary(4),
            ]
        );

        assert_eq!(buffer.len(), 1);
    }

    fn sample_counter(num: usize, tagstr: &str, kind: MetricKind, value: f64) -> Metric {
        Metric::new(
            format!("counter-{}", num),
            None,
            None,
            Some(tag(tagstr)),
            kind,
            MetricValue::Counter { value },
        )
    }

    fn sample_gauge(num: usize, kind: MetricKind, value: f64) -> Metric {
        Metric::new(
            format!("gauge-{}", num),
            None,
            None,
            Some(tag("staging")),
            kind,
            MetricValue::Gauge { value },
        )
    }

    fn sample_set<T: ToString>(num: usize, values: &[T]) -> Metric {
        Metric::new(
            format!("set-{}", num),
            None,
            None,
            Some(tag("production")),
            MetricKind::Incremental,
            MetricValue::Set {
                values: values.iter().map(|s| s.to_string()).collect(),
            },
        )
    }

    fn sample_distribution_histogram(num: u32, rate: u32) -> Metric {
        Metric::new(
            format!("dist-{}", num),
            None,
            None,
            Some(tag("production")),
            MetricKind::Incremental,
            MetricValue::Distribution {
                samples: crate::samples![num as f64 => rate],
                statistic: StatisticKind::Histogram,
            },
        )
    }

    fn sample_aggregated_histogram(
        num: usize,
        kind: MetricKind,
        bpower: f64,
        cfactor: u32,
        sum: f64,
    ) -> Metric {
        Metric::new(
            format!("buckets-{}", num),
            None,
            None,
            Some(tag("production")),
            kind,
            MetricValue::AggregatedHistogram {
                buckets: crate::buckets![
                    1.0 => cfactor,
                    2.0f64.powf(bpower) => cfactor * 2,
                    4.0f64.powf(bpower) => cfactor * 4
                ],
                count: 6 * cfactor,
                sum,
            },
        )
    }

    fn sample_aggregated_summary(factor: u32) -> Metric {
        Metric::new(
            format!("quantiles-{}", factor),
            None,
            None,
            Some(tag("production")),
            MetricKind::Absolute,
            MetricValue::AggregatedSummary {
                quantiles: crate::quantiles![
                    0.0 => factor as f64,
                    0.5 => factor as f64 * 2.0,
                    1.0 => factor as f64 * 4.0
                ],
                count: 6 * factor,
                sum: 10.0,
            },
        )
    }
}
