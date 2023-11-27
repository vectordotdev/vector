use std::cmp::Ordering;

use vector_lib::event::metric::{Metric, MetricValue, Sample};

use crate::sinks::util::{
    batch::{Batch, BatchConfig, BatchError, BatchSize, PushResult},
    Merged, SinkBatchSettings,
};

mod normalize;
pub use self::normalize::*;

mod split;
pub use self::split::*;

/// The metrics buffer is a data structure for collecting a flow of data points into a batch.
///
/// Batching mostly means that we will aggregate away timestamp information, and apply metric-specific compression to
/// improve the performance of the pipeline. In particular, only the latest in a series of metrics are output, and
/// incremental metrics are summed into the output buffer. Any conversion of metrics is handled by the normalization
/// type `N: MetricNormalize`. Further, distribution metrics have their samples compressed with
/// `compress_distribution` below.
///
/// Note: This has been deprecated, please do not use when creating new Sinks.
pub struct MetricsBuffer {
    metrics: Option<MetricSet>,
    max_events: usize,
}

impl MetricsBuffer {
    /// Creates a new `MetricsBuffer` with the given batch settings.
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

    fn get_settings_defaults<D: SinkBatchSettings + Clone>(
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
        // Collect all of our metrics together, finalize them, and hand them back.
        let mut finalized = self
            .metrics
            .map(MetricSet::into_metrics)
            .unwrap_or_default();
        finalized.iter_mut().for_each(finalize_metric);
        finalized
    }

    fn num_items(&self) -> usize {
        self.metrics
            .as_ref()
            .map(|metrics| metrics.len())
            .unwrap_or(0)
    }
}

fn finalize_metric(metric: &mut Metric) {
    if let MetricValue::Distribution { samples, .. } = metric.data_mut().value_mut() {
        let compressed_samples = compress_distribution(samples);
        *samples = compressed_samples;
    }
}

pub fn compress_distribution(samples: &mut Vec<Sample>) -> Vec<Sample> {
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
            acc = *sample;
        }
    }
    result.push(acc);

    result
}

#[cfg(test)]
mod tests {
    use similar_asserts::assert_eq;
    use vector_lib::event::metric::{MetricKind, MetricKind::*, MetricValue, StatisticKind};
    use vector_lib::metric_tags;

    use super::*;
    use crate::{
        sinks::util::BatchSettings,
        test_util::metrics::{AbsoluteMetricNormalizer, IncrementalMetricNormalizer},
    };

    type Buffer = Vec<Vec<Metric>>;

    pub fn sample_counter(num: usize, tagstr: &str, kind: MetricKind, value: f64) -> Metric {
        Metric::new(
            format!("counter-{}", num),
            kind,
            MetricValue::Counter { value },
        )
        .with_tags(Some(metric_tags!(tagstr => "true")))
    }

    pub fn sample_gauge(num: usize, kind: MetricKind, value: f64) -> Metric {
        Metric::new(format!("gauge-{}", num), kind, MetricValue::Gauge { value })
    }

    pub fn sample_set<T: ToString>(num: usize, kind: MetricKind, values: &[T]) -> Metric {
        Metric::new(
            format!("set-{}", num),
            kind,
            MetricValue::Set {
                values: values.iter().map(|s| s.to_string()).collect(),
            },
        )
    }

    pub fn sample_distribution_histogram(num: u32, kind: MetricKind, rate: u32) -> Metric {
        Metric::new(
            format!("dist-{}", num),
            kind,
            MetricValue::Distribution {
                samples: vector_lib::samples![num as f64 => rate],
                statistic: StatisticKind::Histogram,
            },
        )
    }

    pub fn sample_aggregated_histogram(
        num: usize,
        kind: MetricKind,
        bpower: f64,
        cfactor: u64,
        sum: f64,
    ) -> Metric {
        Metric::new(
            format!("buckets-{}", num),
            kind,
            MetricValue::AggregatedHistogram {
                buckets: vector_lib::buckets![
                    1.0 => cfactor,
                    bpower.exp2() => cfactor * 2,
                    4.0f64.powf(bpower) => cfactor * 4
                ],
                count: 7 * cfactor,
                sum,
            },
        )
    }

    pub fn sample_aggregated_summary(num: u32, kind: MetricKind, factor: f64) -> Metric {
        Metric::new(
            format!("quantiles-{}", num),
            kind,
            MetricValue::AggregatedSummary {
                quantiles: vector_lib::quantiles![
                    0.0 => factor,
                    0.5 => factor * 2.0,
                    1.0 => factor * 4.0
                ],
                count: factor as u64 * 10,
                sum: factor * 7.0,
            },
        )
    }

    fn rebuffer<State: MetricNormalize + Default>(metrics: Vec<Metric>) -> Buffer {
        let mut batch_settings = BatchSettings::default();
        batch_settings.size.bytes = 9999;
        batch_settings.size.events = 6;

        let mut normalizer = MetricNormalizer::<State>::default();
        let mut buffer = MetricsBuffer::new(batch_settings.size);
        let mut result = vec![];

        for metric in metrics {
            if let Some(event) = normalizer.normalize(metric) {
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

    fn rebuffer_incremental_counters<State: MetricNormalize + Default>() -> Buffer {
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
        let buffer = rebuffer_incremental_counters::<AbsoluteMetricNormalizer>();

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
        let buffer = rebuffer_incremental_counters::<IncrementalMetricNormalizer>();

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

    fn rebuffer_absolute_counters<State: MetricNormalize + Default>() -> Buffer {
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
        let buffer = rebuffer_absolute_counters::<AbsoluteMetricNormalizer>();

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
        let buffer = rebuffer_absolute_counters::<IncrementalMetricNormalizer>();

        assert_eq!(
            buffer[0],
            [
                sample_counter(2, "production", Incremental, 4.0),
                sample_counter(3, "production", Incremental, 6.0),
            ]
        );

        assert_eq!(buffer.len(), 1);
    }

    fn rebuffer_incremental_gauges<State: MetricNormalize + Default>() -> Buffer {
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
        let buffer = rebuffer_incremental_gauges::<AbsoluteMetricNormalizer>();

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
        let buffer = rebuffer_incremental_gauges::<IncrementalMetricNormalizer>();

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

    fn rebuffer_absolute_gauges<State: MetricNormalize + Default>() -> Buffer {
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
        let buffer = rebuffer_absolute_gauges::<AbsoluteMetricNormalizer>();

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
        let buffer = rebuffer_absolute_gauges::<IncrementalMetricNormalizer>();

        assert_eq!(
            buffer[0],
            [
                sample_gauge(3, Incremental, 24.0),
                sample_gauge(4, Incremental, 32.0),
            ]
        );

        assert_eq!(buffer.len(), 1);
    }

    fn rebuffer_incremental_sets<State: MetricNormalize + Default>() -> Buffer {
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
        let buffer = rebuffer_incremental_sets::<AbsoluteMetricNormalizer>();

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
        let buffer = rebuffer_incremental_sets::<IncrementalMetricNormalizer>();

        assert_eq!(
            buffer[0],
            [
                sample_set(0, Incremental, &[0, 1, 2, 3]),
                sample_set(1, Incremental, &[1, 2, 3, 4]),
            ]
        );

        assert_eq!(buffer.len(), 1);
    }

    fn rebuffer_incremental_distributions<State: MetricNormalize + Default>() -> Buffer {
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
        let buffer = rebuffer_incremental_distributions::<AbsoluteMetricNormalizer>();

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
        let buffer = rebuffer_incremental_distributions::<IncrementalMetricNormalizer>();

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
        let mut samples = vector_lib::samples![
            2.0 => 12,
            2.0 => 12,
            3.0 => 13,
            1.0 => 11,
            2.0 => 12,
            2.0 => 12,
            3.0 => 13
        ];

        assert_eq!(
            compress_distribution(&mut samples),
            vector_lib::samples![1.0 => 11, 2.0 => 48, 3.0 => 26]
        );
    }

    fn rebuffer_absolute_aggregated_histograms<State: MetricNormalize + Default>() -> Buffer {
        let mut events = Vec::new();
        for _ in 2..5 {
            events.push(sample_aggregated_histogram(2, Absolute, 1.0, 1, 10.0));
        }

        for i in 2..5 {
            events.push(sample_aggregated_histogram(
                i,
                Absolute,
                1.0,
                i as u64,
                i as f64 * 10.0,
            ));
        }

        rebuffer::<State>(events)
    }

    #[test]
    fn abs_buffer_absolute_aggregated_histograms() {
        let buffer = rebuffer_absolute_aggregated_histograms::<AbsoluteMetricNormalizer>();

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
        let buffer = rebuffer_absolute_aggregated_histograms::<IncrementalMetricNormalizer>();

        assert_eq!(
            buffer[0],
            [sample_aggregated_histogram(2, Incremental, 1.0, 1, 10.0)]
        );

        assert_eq!(buffer.len(), 1);
    }

    fn rebuffer_incremental_aggregated_histograms<State: MetricNormalize + Default>() -> Buffer {
        let mut events = vec![sample_aggregated_histogram(2, Incremental, 1.0, 1, 10.0)];

        for i in 1..4 {
            events.push(sample_aggregated_histogram(2, Incremental, 2.0, i, 10.0));
        }

        rebuffer::<State>(events)
    }

    #[test]
    fn abs_buffer_incremental_aggregated_histograms() {
        let buffer = rebuffer_incremental_aggregated_histograms::<AbsoluteMetricNormalizer>();

        assert_eq!(
            buffer[0],
            [sample_aggregated_histogram(2, Absolute, 2.0, 6, 30.0)]
        );

        assert_eq!(buffer.len(), 1);
    }

    #[test]
    fn inc_buffer_incremental_aggregated_histograms() {
        let buffer = rebuffer_incremental_aggregated_histograms::<IncrementalMetricNormalizer>();

        assert_eq!(
            buffer[0],
            [sample_aggregated_histogram(2, Incremental, 2.0, 6, 30.0)]
        );

        assert_eq!(buffer.len(), 1);
    }

    fn rebuffer_aggregated_summaries<State: MetricNormalize + Default>() -> Buffer {
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
        let buffer = rebuffer_aggregated_summaries::<AbsoluteMetricNormalizer>();

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
        let buffer = rebuffer_aggregated_summaries::<IncrementalMetricNormalizer>();

        // Since aggregated summaries cannot be added, they don't work
        // as incremental metrics and this results in an empty buffer.
        assert_eq!(buffer.len(), 0);
    }
}
