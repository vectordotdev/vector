use std::{cmp::Ordering, collections::HashMap, marker::PhantomData};

use vector_core::event::{
    metric::{Metric, MetricData, MetricKind, MetricSeries, MetricValue, Sample},
    Event, EventMetadata,
};

use crate::sinks::util::{
    batch::{Batch, BatchConfig, BatchError, BatchSize, PushResult},
    Merged, SinkBatchSettings,
};

/// The metrics buffer is a data structure for collecting a flow of data
/// points into a batch.
///
/// Batching mostly means that we will aggregate away timestamp
/// information, and apply metric-specific compression to improve the
/// performance of the pipeline. In particular, only the latest in a
/// series of metrics are output, and incremental metrics are summed
/// into the output buffer. Any conversion of metrics is handled by the
/// normalization type `N: MetricNormalize`. Further, distribution
/// metrics have their their samples compressed with
/// `compress_distribution` below.
pub struct MetricsBuffer {
    metrics: Option<MetricSet>,
    max_events: usize,
}

impl MetricsBuffer {
    pub const fn new(settings: BatchSize<Self>) -> Self {
        Self::with_capacity(settings.events)
    }

    const fn with_capacity(max_events: usize) -> Self {
        Self {
            metrics: None,
            max_events,
        }
    }
}

impl Batch for MetricsBuffer {
    type Input = Metric;
    type Output = Vec<Metric>;

    fn get_settings_defaults<D: SinkBatchSettings>(
        config: BatchConfig<D, Merged>,
    ) -> Result<BatchConfig<D, Merged>, BatchError> {
        config.disallow_max_bytes()
    }

    fn push(&mut self, item: Self::Input) -> PushResult<Self::Input> {
        if self.num_items() >= self.max_events {
            PushResult::Overflow(item)
        } else {
            let max_events = self.max_events;
            self.metrics
                .get_or_insert_with(|| MetricSet::with_capacity(max_events))
                .insert_update(item);
            PushResult::Ok(self.num_items() >= self.max_events)
        }
    }

    fn is_empty(&self) -> bool {
        self.num_items() == 0
    }

    fn fresh(&self) -> Self {
        Self::with_capacity(self.max_events)
    }

    fn finish(self) -> Self::Output {
        self.metrics
            .unwrap_or_else(|| MetricSet::with_capacity(0))
            .0
            .into_iter()
            .map(finish_metric)
            .collect()
    }

    fn num_items(&self) -> usize {
        self.metrics
            .as_ref()
            .map(|metrics| metrics.0.len())
            .unwrap_or(0)
    }
}

/// This is a simple wrapper for using `MetricNormalize` with a
/// persistent `MetricSet` state, to be used in sinks in `with_flat_map`
/// before sending the events to the `MetricsBuffer`
pub struct MetricNormalizer<N> {
    state: MetricSet,
    _norm: PhantomData<N>,
}

impl<N: MetricNormalize> MetricNormalizer<N> {
    pub fn default() -> Self {
        Self {
            state: MetricSet::default(),
            _norm: PhantomData::default(),
        }
    }

    /// This wraps `MetricNormalize::apply_state`, converting to/from
    /// the `Metric` type wrapper. See that function for return values.
    pub fn apply(&mut self, event: Event) -> Option<Metric> {
        N::apply_state(&mut self.state, event.into_metric())
    }
}

/// The metrics state trait abstracts how data point normalization is
/// done. Normalisation is required to make sure Sources and Sinks are
/// exchanging compatible data structures. For instance, delta gauges
/// produced by Statsd source cannot be directly sent to Datadog API. In
/// this case the buffer will keep the state of a gauge value, and
/// produce absolute values gauges that are well supported by Datadog.
///
/// Another example of normalisation is disaggregation of counters. Most
/// sinks would expect we send them delta counters (e.g. how many events
/// occurred during the flush period). And most sources are producing
/// exactly these kind of counters, with Prometheus being a notable
/// exception. If the counter comes already aggregated inside the
/// source, the buffer will compare it's values with the previous known
/// and calculate the delta.
pub trait MetricNormalize {
    /// Apply normalizes the given `metric` using `state` to save any
    /// persistent data between calls. The return value is `None` if the
    /// incoming metric is only used to set a reference state, and
    /// `Some(metric)` otherwise.
    fn apply_state(state: &mut MetricSet, metric: Metric) -> Option<Metric>;
}

type MetricEntry = (MetricData, EventMetadata);

/// This is a convenience wrapper for HashMap<MetricSeries, MetricData>
/// that provides some extra functionality.
#[derive(Clone, Default)]
pub struct MetricSet(HashMap<MetricSeries, MetricEntry>);

impl MetricSet {
    fn with_capacity(capacity: usize) -> Self {
        Self(HashMap::with_capacity(capacity))
    }

    /// Either pass the metric through as-is if absolute, or convert it
    /// to absolute if incremental.
    pub fn make_absolute(&mut self, metric: Metric) -> Option<Metric> {
        match metric.kind() {
            MetricKind::Absolute => Some(metric),
            MetricKind::Incremental => Some(self.incremental_to_absolute(metric)),
        }
    }

    /// Either convert the metric to incremental if absolute, or
    /// aggregate it with any previous value if already incremental.
    pub fn make_incremental(&mut self, metric: Metric) -> Option<Metric> {
        match metric.kind() {
            MetricKind::Absolute => self.absolute_to_incremental(metric),
            MetricKind::Incremental => Some(metric),
        }
    }

    /// Convert the incremental metric into an absolute one, using the
    /// state buffer to keep track of the value throughout the entire
    /// application uptime.
    fn incremental_to_absolute(&mut self, mut metric: Metric) -> Metric {
        match self.0.get_mut(metric.series()) {
            Some(existing) => {
                if existing.0.value.add(metric.value()) {
                    metric = metric.with_value(existing.0.value.clone());
                } else {
                    // Metric changed type, store this as the new reference value
                    self.0.insert(
                        metric.series().clone(),
                        (metric.data().clone(), EventMetadata::default()),
                    );
                }
            }
            None => {
                self.0.insert(
                    metric.series().clone(),
                    (metric.data().clone(), EventMetadata::default()),
                );
            }
        }
        metric.into_absolute()
    }

    /// Convert the absolute metric into an incremental by calculating
    /// the increment from the last saved absolute state.
    fn absolute_to_incremental(&mut self, mut metric: Metric) -> Option<Metric> {
        // NOTE: Crucially, like I did, you may wonder: why do we not always return a metric? Could
        // this lead to issues where a metric isn't seen again and we, in effect, never emit it?
        //
        // You're not wrong, and that does happen based on the logic below.  However, the main
        // problem this logic solves is avoiding massive counter updates when Vector restarts.
        //
        // If we emitted a metric for a newly-seen absolute metric in this method, we would
        // naturally have to emit an incremental version where the value was the absolute value,
        // with subsequent updates being only delta updates.  If we restarted Vector, however, we
        // would be back to not having yet seen the metric before, so the first emission of the
        // metric after converting it here would be... its absolute value.  Even if the value only
        // changed by 1 between Vector stopping and restarting, we could be incrementing the counter
        // by some outrageous amount.
        //
        // Thus, we only emit a metric when we've calculated an actual delta for it, which means
        // that, yes, we're risking never seeing a metric if it's not re-emitted, and we're
        // introducing a small amount of lag before a metric is emitted by having to wait to see it
        // again, but this is a behavior we have to observe for sinks that can only handle
        // incremental updates.
        match self.0.get_mut(metric.series()) {
            Some(reference) => {
                let new_value = metric.value().clone();
                // From the stored reference value, emit an increment
                if metric.subtract(&reference.0) {
                    reference.0.value = new_value;
                    Some(metric.into_incremental())
                } else {
                    // Metric changed type, store this and emit nothing
                    self.insert(metric);
                    None
                }
            }
            None => {
                // No reference so store this and emit nothing
                self.insert(metric);
                None
            }
        }
    }

    fn insert(&mut self, metric: Metric) {
        let (series, data, metadata) = metric.into_parts();
        self.0.insert(series, (data, metadata));
    }

    fn insert_update(&mut self, metric: Metric) {
        let update = match metric.kind() {
            MetricKind::Absolute => Some(metric),
            MetricKind::Incremental => {
                // Incremental metrics update existing entries, if present
                match self.0.get_mut(metric.series()) {
                    Some(existing) => {
                        let (series, data, metadata) = metric.into_parts();
                        if existing.0.update(&data) {
                            existing.1.merge(metadata);
                            None
                        } else {
                            warn!(message = "Metric changed type, dropping old value.", %series);
                            Some(Metric::from_parts(series, data, metadata))
                        }
                    }
                    None => Some(metric),
                }
            }
        };
        if let Some(metric) = update {
            self.insert(metric);
        }
    }
}

fn finish_metric(item: (MetricSeries, MetricEntry)) -> Metric {
    let (series, (mut data, metadata)) = item;
    if let MetricValue::Distribution { samples, statistic } = data.value {
        let samples = compress_distribution(samples);
        data.value = MetricValue::Distribution { samples, statistic };
    }
    Metric::from_parts(series, data, metadata)
}

pub fn compress_distribution(mut samples: Vec<Sample>) -> Vec<Sample> {
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
    use std::collections::BTreeMap;

    use pretty_assertions::assert_eq;

    use super::*;
    use crate::{
        event::metric::{MetricKind::*, MetricValue, StatisticKind},
        sinks::util::BatchSettings,
    };

    type Buffer = Vec<Vec<Metric>>;

    struct AbsoluteMetricNormalize;

    impl MetricNormalize for AbsoluteMetricNormalize {
        fn apply_state(state: &mut MetricSet, metric: Metric) -> Option<Metric> {
            state.make_absolute(metric)
        }
    }

    struct IncrementalMetricNormalize;

    impl MetricNormalize for IncrementalMetricNormalize {
        fn apply_state(state: &mut MetricSet, metric: Metric) -> Option<Metric> {
            state.make_incremental(metric)
        }
    }

    fn tag(name: &str) -> BTreeMap<String, String> {
        vec![(name.to_owned(), "true".to_owned())]
            .into_iter()
            .collect()
    }

    fn rebuffer<State: MetricNormalize>(metrics: Vec<Metric>) -> Buffer {
        let mut batch_settings = BatchSettings::default();
        batch_settings.size.bytes = 9999;
        batch_settings.size.events = 6;

        let mut normalizer = MetricNormalizer::<State>::default();
        let mut buffer = MetricsBuffer::new(batch_settings.size);
        let mut result = vec![];

        for metric in metrics {
            if let Some(event) = normalizer.apply(Event::Metric(metric)) {
                match buffer.push(event) {
                    PushResult::Overflow(_) => panic!("overflowed too early"),
                    PushResult::Ok(true) => {
                        let batch =
                            std::mem::replace(&mut buffer, MetricsBuffer::new(batch_settings.size));
                        result.push(batch.finish());
                    }
                    PushResult::Ok(false) => (),
                }
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

    fn rebuffer_incremental_counters<State: MetricNormalize>() -> Buffer {
        let mut events = Vec::new();
        for i in 0..4 {
            // counter-0 is repeated 5 times
            events.push(sample_counter(0, "production", Incremental, i as f64));
        }

        for i in 0..4 {
            // these counters cause a buffer flush
            events.push(sample_counter(i, "staging", Incremental, i as f64));
        }

        for i in 0..4 {
            // counter-0 increments the previous buffer, the rest are new
            events.push(sample_counter(i, "production", Incremental, i as f64));
        }

        rebuffer::<State>(events)
    }

    #[test]
    fn abs_buffer_incremental_counters() {
        let buffer = rebuffer_incremental_counters::<AbsoluteMetricNormalize>();

        assert_eq!(
            buffer[0],
            [
                sample_counter(0, "production", Absolute, 6.0),
                sample_counter(0, "staging", Absolute, 0.0),
                sample_counter(1, "production", Absolute, 1.0),
                sample_counter(1, "staging", Absolute, 1.0),
                sample_counter(2, "staging", Absolute, 2.0),
                sample_counter(3, "staging", Absolute, 3.0),
            ]
        );

        assert_eq!(
            buffer[1],
            [
                sample_counter(2, "production", Absolute, 2.0),
                sample_counter(3, "production", Absolute, 3.0),
            ]
        );

        assert_eq!(buffer.len(), 2);
    }

    #[test]
    fn inc_buffer_incremental_counters() {
        let buffer = rebuffer_incremental_counters::<IncrementalMetricNormalize>();

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

    fn rebuffer_absolute_counters<State: MetricNormalize>() -> Buffer {
        let mut events = Vec::new();
        // counter-0 and -1 only emitted once
        // counter-2 and -3 emitted twice
        // counter-4 and -5 emitted once
        for i in 0..4 {
            events.push(sample_counter(i, "production", Absolute, i as f64));
        }

        for i in 2..6 {
            events.push(sample_counter(i, "production", Absolute, i as f64 * 3.0));
        }

        rebuffer::<State>(events)
    }

    #[test]
    fn abs_buffer_absolute_counters() {
        let buffer = rebuffer_absolute_counters::<AbsoluteMetricNormalize>();

        assert_eq!(
            buffer[0],
            [
                sample_counter(0, "production", Absolute, 0.0),
                sample_counter(1, "production", Absolute, 1.0),
                sample_counter(2, "production", Absolute, 6.0),
                sample_counter(3, "production", Absolute, 9.0),
                sample_counter(4, "production", Absolute, 12.0),
                sample_counter(5, "production", Absolute, 15.0),
            ]
        );

        assert_eq!(buffer.len(), 1);
    }

    #[test]
    fn inc_buffer_absolute_counters() {
        let buffer = rebuffer_absolute_counters::<IncrementalMetricNormalize>();

        assert_eq!(
            buffer[0],
            [
                sample_counter(2, "production", Incremental, 4.0),
                sample_counter(3, "production", Incremental, 6.0),
            ]
        );

        assert_eq!(buffer.len(), 1);
    }

    fn rebuffer_incremental_gauges<State: MetricNormalize>() -> Buffer {
        let mut events = Vec::new();
        // gauge-1 emitted once
        // gauge-2 through -4 are emitted twice
        // gauge-5 emitted once
        for i in 1..5 {
            events.push(sample_gauge(i, Incremental, i as f64));
        }

        for i in 2..6 {
            events.push(sample_gauge(i, Incremental, i as f64));
        }

        rebuffer::<State>(events)
    }

    #[test]
    fn abs_buffer_incremental_gauges() {
        let buffer = rebuffer_incremental_gauges::<AbsoluteMetricNormalize>();

        assert_eq!(
            buffer[0],
            [
                sample_gauge(1, Absolute, 1.0),
                sample_gauge(2, Absolute, 4.0),
                sample_gauge(3, Absolute, 6.0),
                sample_gauge(4, Absolute, 8.0),
                sample_gauge(5, Absolute, 5.0),
            ]
        );

        assert_eq!(buffer.len(), 1);
    }

    #[test]
    fn inc_buffer_incremental_gauges() {
        let buffer = rebuffer_incremental_gauges::<IncrementalMetricNormalize>();

        assert_eq!(
            buffer[0],
            [
                sample_gauge(1, Incremental, 1.0),
                sample_gauge(2, Incremental, 4.0),
                sample_gauge(3, Incremental, 6.0),
                sample_gauge(4, Incremental, 8.0),
                sample_gauge(5, Incremental, 5.0),
            ]
        );

        assert_eq!(buffer.len(), 1);
    }

    fn rebuffer_absolute_gauges<State: MetricNormalize>() -> Buffer {
        let mut events = Vec::new();
        // gauge-2 emitted once
        // gauge-3 and -4 emitted twice
        // gauge-5 emitted once
        for i in 2..5 {
            events.push(sample_gauge(i, Absolute, i as f64 * 2.0));
        }

        for i in 3..6 {
            events.push(sample_gauge(i, Absolute, i as f64 * 10.0));
        }

        rebuffer::<State>(events)
    }

    #[test]
    fn abs_buffer_absolute_gauges() {
        let buffer = rebuffer_absolute_gauges::<AbsoluteMetricNormalize>();

        assert_eq!(
            buffer[0],
            [
                sample_gauge(2, Absolute, 4.0),
                sample_gauge(3, Absolute, 30.0),
                sample_gauge(4, Absolute, 40.0),
                sample_gauge(5, Absolute, 50.0),
            ]
        );

        assert_eq!(buffer.len(), 1);
    }

    #[test]
    fn inc_buffer_absolute_gauges() {
        let buffer = rebuffer_absolute_gauges::<IncrementalMetricNormalize>();

        assert_eq!(
            buffer[0],
            [
                sample_gauge(3, Incremental, 24.0),
                sample_gauge(4, Incremental, 32.0),
            ]
        );

        assert_eq!(buffer.len(), 1);
    }

    fn rebuffer_incremental_sets<State: MetricNormalize>() -> Buffer {
        let mut events = Vec::new();
        // set-0 emitted 8 times with 4 different values
        // set-1 emitted once with 4 values
        for i in 0..4 {
            events.push(sample_set(0, Incremental, &[i]));
        }

        for i in 0..4 {
            events.push(sample_set(0, Incremental, &[i]));
        }

        events.push(sample_set(1, Incremental, &[1, 2, 3, 4]));

        rebuffer::<State>(events)
    }

    #[test]
    fn abs_buffer_incremental_sets() {
        let buffer = rebuffer_incremental_sets::<AbsoluteMetricNormalize>();

        assert_eq!(
            buffer[0],
            [
                sample_set(0, Absolute, &[0, 1, 2, 3]),
                sample_set(1, Absolute, &[1, 2, 3, 4]),
            ]
        );

        assert_eq!(buffer.len(), 1);
    }

    #[test]
    fn inc_buffer_incremental_sets() {
        let buffer = rebuffer_incremental_sets::<IncrementalMetricNormalize>();

        assert_eq!(
            buffer[0],
            [
                sample_set(0, Incremental, &[0, 1, 2, 3]),
                sample_set(1, Incremental, &[1, 2, 3, 4]),
            ]
        );

        assert_eq!(buffer.len(), 1);
    }

    fn rebuffer_incremental_distributions<State: MetricNormalize>() -> Buffer {
        let mut events = Vec::new();
        for _ in 2..6 {
            events.push(sample_distribution_histogram(2, Incremental, 10));
        }

        for i in 2..6 {
            events.push(sample_distribution_histogram(i, Incremental, 10));
        }

        rebuffer::<State>(events)
    }

    #[test]
    fn abs_buffer_incremental_distributions() {
        let buffer = rebuffer_incremental_distributions::<AbsoluteMetricNormalize>();

        assert_eq!(
            buffer[0],
            [
                sample_distribution_histogram(2, Absolute, 50),
                sample_distribution_histogram(3, Absolute, 10),
                sample_distribution_histogram(4, Absolute, 10),
                sample_distribution_histogram(5, Absolute, 10),
            ]
        );

        assert_eq!(buffer.len(), 1);
    }

    #[test]
    fn inc_buffer_incremental_distributions() {
        let buffer = rebuffer_incremental_distributions::<IncrementalMetricNormalize>();

        assert_eq!(
            buffer[0],
            [
                sample_distribution_histogram(2, Incremental, 50),
                sample_distribution_histogram(3, Incremental, 10),
                sample_distribution_histogram(4, Incremental, 10),
                sample_distribution_histogram(5, Incremental, 10),
            ]
        );

        assert_eq!(buffer.len(), 1);
    }

    #[test]
    fn compress_distributions() {
        let samples = vector_core::samples![
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
            vector_core::samples![1.0 => 11, 2.0 => 48, 3.0 => 26]
        );
    }

    fn rebuffer_absolute_aggregated_histograms<State: MetricNormalize>() -> Buffer {
        let mut events = Vec::new();
        for _ in 2..5 {
            events.push(sample_aggregated_histogram(2, Absolute, 1.0, 1, 10.0));
        }

        for i in 2..5 {
            events.push(sample_aggregated_histogram(
                i,
                Absolute,
                1.0,
                i as u32,
                i as f64 * 10.0,
            ));
        }

        rebuffer::<State>(events)
    }

    #[test]
    fn abs_buffer_absolute_aggregated_histograms() {
        let buffer = rebuffer_absolute_aggregated_histograms::<AbsoluteMetricNormalize>();

        assert_eq!(
            buffer[0],
            [
                sample_aggregated_histogram(2, Absolute, 1.0, 2, 20.0),
                sample_aggregated_histogram(3, Absolute, 1.0, 3, 30.0),
                sample_aggregated_histogram(4, Absolute, 1.0, 4, 40.0),
            ]
        );

        assert_eq!(buffer.len(), 1);
    }

    #[test]
    fn inc_buffer_absolute_aggregated_histograms() {
        let buffer = rebuffer_absolute_aggregated_histograms::<IncrementalMetricNormalize>();

        assert_eq!(
            buffer[0],
            [sample_aggregated_histogram(2, Incremental, 1.0, 1, 10.0)]
        );

        assert_eq!(buffer.len(), 1);
    }

    fn rebuffer_incremental_aggregated_histograms<State: MetricNormalize>() -> Buffer {
        let mut events = vec![sample_aggregated_histogram(2, Incremental, 1.0, 1, 10.0)];

        for i in 1..4 {
            events.push(sample_aggregated_histogram(2, Incremental, 2.0, i, 10.0));
        }

        rebuffer::<State>(events)
    }

    #[test]
    fn abs_buffer_incremental_aggregated_histograms() {
        let buffer = rebuffer_incremental_aggregated_histograms::<AbsoluteMetricNormalize>();

        assert_eq!(
            buffer[0],
            [sample_aggregated_histogram(2, Absolute, 2.0, 6, 30.0)]
        );

        assert_eq!(buffer.len(), 1);
    }

    #[test]
    fn inc_buffer_incremental_aggregated_histograms() {
        let buffer = rebuffer_incremental_aggregated_histograms::<IncrementalMetricNormalize>();

        assert_eq!(
            buffer[0],
            [sample_aggregated_histogram(2, Incremental, 2.0, 6, 30.0)]
        );

        assert_eq!(buffer.len(), 1);
    }

    fn rebuffer_aggregated_summaries<State: MetricNormalize>() -> Buffer {
        let mut events = Vec::new();
        for factor in 0..2 {
            for num in 2..4 {
                events.push(sample_aggregated_summary(
                    num,
                    Absolute,
                    (factor + num) as f64,
                ));
            }
        }

        rebuffer::<State>(events)
    }

    #[test]
    fn abs_buffer_aggregated_summaries() {
        let buffer = rebuffer_aggregated_summaries::<AbsoluteMetricNormalize>();

        assert_eq!(
            buffer[0],
            [
                sample_aggregated_summary(2, Absolute, 3.0),
                sample_aggregated_summary(3, Absolute, 4.0),
            ]
        );

        assert_eq!(buffer.len(), 1);
    }

    #[test]
    fn inc_buffer_aggregated_summaries() {
        let buffer = rebuffer_aggregated_summaries::<IncrementalMetricNormalize>();

        // Since aggregated summaries cannot be added, they don't work
        // as incremental metrics and this results in an empty buffer.
        assert_eq!(buffer.len(), 0);
    }

    fn sample_counter(num: usize, tagstr: &str, kind: MetricKind, value: f64) -> Metric {
        Metric::new(
            format!("counter-{}", num),
            kind,
            MetricValue::Counter { value },
        )
        .with_tags(Some(tag(tagstr)))
    }

    fn sample_gauge(num: usize, kind: MetricKind, value: f64) -> Metric {
        Metric::new(format!("gauge-{}", num), kind, MetricValue::Gauge { value })
    }

    fn sample_set<T: ToString>(num: usize, kind: MetricKind, values: &[T]) -> Metric {
        Metric::new(
            format!("set-{}", num),
            kind,
            MetricValue::Set {
                values: values.iter().map(|s| s.to_string()).collect(),
            },
        )
    }

    fn sample_distribution_histogram(num: u32, kind: MetricKind, rate: u32) -> Metric {
        Metric::new(
            format!("dist-{}", num),
            kind,
            MetricValue::Distribution {
                samples: vector_core::samples![num as f64 => rate],
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
            kind,
            MetricValue::AggregatedHistogram {
                buckets: vector_core::buckets![
                    1.0 => cfactor,
                    bpower.exp2() => cfactor * 2,
                    4.0f64.powf(bpower) => cfactor * 4
                ],
                count: 7 * cfactor,
                sum,
            },
        )
    }

    fn sample_aggregated_summary(num: u32, kind: MetricKind, factor: f64) -> Metric {
        Metric::new(
            format!("quantiles-{}", num),
            kind,
            MetricValue::AggregatedSummary {
                quantiles: vector_core::quantiles![
                    0.0 => factor,
                    0.5 => factor * 2.0,
                    1.0 => factor * 4.0
                ],
                count: factor as u32 * 10,
                sum: factor * 7.0,
            },
        )
    }
}
