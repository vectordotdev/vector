use std::collections::{HashMap, HashSet};

use std::fmt::Display;

use vector_lib::event::{
    metric::{Bucket, MetricData, MetricSeries, Sample},
    Event, EventMetadata, Metric, MetricValue, StatisticKind,
};

use crate::event::MetricKind;
use crate::sinks::util::buffer::metrics::{MetricNormalize, MetricSet};

type SplitMetrics = HashMap<MetricSeries, (MetricData, EventMetadata)>;
pub type AbsoluteMetricState = MetricState<AbsoluteMetricNormalizer>;
pub type IncrementalMetricState = MetricState<IncrementalMetricNormalizer>;

#[derive(Default)]
pub struct AbsoluteMetricNormalizer;

impl MetricNormalize for AbsoluteMetricNormalizer {
    fn normalize(&mut self, state: &mut MetricSet, metric: Metric) -> Option<Metric> {
        state.make_absolute(metric)
    }
}

#[derive(Default)]
pub struct IncrementalMetricNormalizer;

impl MetricNormalize for IncrementalMetricNormalizer {
    fn normalize(&mut self, state: &mut MetricSet, metric: Metric) -> Option<Metric> {
        state.make_incremental(metric)
    }
}

pub struct MetricState<N> {
    intermediate: MetricSet,
    normalizer: N,
    latest: HashMap<MetricSeries, (MetricData, EventMetadata)>,
}

impl<N: MetricNormalize> MetricState<N> {
    pub fn merge(&mut self, metric: Metric) {
        if let Some(output) = self.normalizer.normalize(&mut self.intermediate, metric) {
            let (series, data, metadata) = output.into_parts();
            self.latest.insert(series, (data, metadata));
        }
    }

    pub fn finish(self) -> SplitMetrics {
        let mut latest = self.latest;

        // If we had an absolute value stored in the normalizer state that was never
        // updated/seen more than once, we will never have gotten it back from the `apply_state`
        // call, so we're adding all items in the normalizer state that aren't already present
        // in the latest map.
        for metric in self.intermediate.into_metrics() {
            if !latest.contains_key(metric.series()) {
                let (series, data, metadata) = metric.into_parts();
                latest.insert(series, (data, metadata));
            }
        }

        latest
    }
}

impl<N: MetricNormalize> Extend<Event> for MetricState<N> {
    fn extend<T: IntoIterator<Item = Event>>(&mut self, iter: T) {
        for event in iter.into_iter() {
            self.merge(event.into_metric());
        }
    }
}

impl<N: MetricNormalize + Default> FromIterator<Event> for MetricState<N> {
    fn from_iter<T: IntoIterator<Item = Event>>(iter: T) -> Self {
        let mut state = MetricState::default();
        for event in iter.into_iter() {
            state.merge(event.into_metric());
        }
        state
    }
}

impl<N> From<N> for MetricState<N> {
    fn from(normalizer: N) -> Self {
        Self {
            intermediate: MetricSet::default(),
            normalizer,
            latest: HashMap::default(),
        }
    }
}

impl<N: Default> Default for MetricState<N> {
    fn default() -> Self {
        Self {
            intermediate: MetricSet::default(),
            normalizer: N::default(),
            latest: HashMap::default(),
        }
    }
}

pub fn read_counter_value(metrics: &SplitMetrics, series: MetricSeries) -> Option<f64> {
    metrics
        .get(&series)
        .and_then(|(data, _)| match data.value() {
            MetricValue::Counter { value } => Some(*value),
            _ => None,
        })
}

pub fn read_gauge_value(metrics: &SplitMetrics, series: MetricSeries) -> Option<f64> {
    metrics
        .get(&series)
        .and_then(|(data, _)| match data.value() {
            MetricValue::Gauge { value } => Some(*value),
            _ => None,
        })
}

pub fn read_distribution_samples(
    metrics: &SplitMetrics,
    series: MetricSeries,
) -> Option<Vec<Sample>> {
    metrics
        .get(&series)
        .and_then(|(data, _)| match data.value() {
            MetricValue::Distribution { samples, .. } => Some(samples.clone()),
            _ => None,
        })
}

pub fn read_set_values(metrics: &SplitMetrics, series: MetricSeries) -> Option<HashSet<String>> {
    metrics
        .get(&series)
        .and_then(|(data, _)| match data.value() {
            MetricValue::Set { values } => Some(values.iter().cloned().collect()),
            _ => None,
        })
}

#[macro_export]
macro_rules! series {
	($name:expr) => {
		vector_lib::event::metric::MetricSeries {
			name: vector_lib::event::metric::MetricName {
				name: $name.into(),
				namespace: None,
			},
			tags: None,
		}
	};
	($name:expr, $($tk:expr => $tv:expr),*) => {
		vector_lib::event::metric::MetricSeries {
			name: vector_lib::event::metric::MetricName {
				name: $name.into(),
				namespace: None,
			},
			tags: Some(vector_lib::metric_tags!( $( $tk => $tv, )* )),
		}
	};
}

pub fn assert_counter(metrics: &SplitMetrics, series: MetricSeries, expected: f64) {
    let actual_counter = read_counter_value(metrics, series.clone());
    assert!(
        actual_counter.is_some(),
        "counter '{}' was not found",
        series
    );

    let actual_counter_value = actual_counter.expect("counter must be valid");
    assert_eq!(
        actual_counter_value, expected,
        "expected {} for '{}', got {} instead",
        expected, series, actual_counter_value
    );
}

pub fn assert_gauge(metrics: &SplitMetrics, series: MetricSeries, expected: f64) {
    let actual_gauge = read_gauge_value(metrics, series.clone());
    assert!(actual_gauge.is_some(), "gauge '{}' was not found", series);

    let actual_gauge_value = actual_gauge.expect("gauge must be valid");
    assert_eq!(
        actual_gauge_value, expected,
        "expected {} for '{}', got {} instead",
        expected, series, actual_gauge_value
    );
}

pub fn assert_distribution(
    metrics: &SplitMetrics,
    series: MetricSeries,
    expected_sum: f64,
    expected_count: u32,
    expected_bounds: &[(f64, u32)],
) {
    let samples = read_distribution_samples(metrics, series.clone());
    assert!(samples.is_some(), "distribution '{}' was not found", series);

    let samples = samples.expect("distribution must be valid");

    let mut actual_sum = 0.0;
    let mut actual_count = 0;
    let mut actual_bounds = vec![0u32; expected_bounds.len()];
    for sample in &samples {
        actual_sum += sample.rate as f64 * sample.value;
        actual_count += sample.rate;

        for (i, (bound, _)) in expected_bounds.iter().enumerate() {
            if sample.value <= *bound {
                actual_bounds[i] += sample.rate;
            }
        }
    }

    assert_eq!(
        actual_sum, expected_sum,
        "expected sum of '{}' to equal {}, got {} instead",
        series, expected_sum, actual_sum
    );
    assert_eq!(
        actual_count, expected_count,
        "expected count of '{}' to equal {}, got {} instead",
        series, expected_count, actual_count
    );

    for (i, (bound, count)) in expected_bounds.iter().enumerate() {
        assert_eq!(
            actual_bounds[i], *count,
            "expected {} samples less than or equal to {} for '{}', found {} instead",
            count, bound, series, actual_bounds[i]
        );
    }
}

pub fn assert_set(metrics: &SplitMetrics, series: MetricSeries, expected_values: &[&str]) {
    let actual_values = read_set_values(metrics, series.clone());
    assert!(actual_values.is_some(), "set '{}' was not found", series);

    let actual_values = actual_values.expect("set must be valid");
    let expected_values = expected_values
        .iter()
        .map(|s| s.to_string())
        .collect::<HashSet<_>>();

    assert_eq!(actual_values, expected_values);
}

fn buckets_from_samples(values: &[f64]) -> (Vec<Bucket>, f64, u64) {
    // Generate buckets, and general statistics, for an input set of data.  We only use this in
    // tests, and so we have some semi-realistic buckets here, but mainly we use them for testing,
    // not for most accurately/efficiently representing the input samples.
    let bounds = &[
        1.0,
        2.0,
        4.0,
        8.0,
        16.0,
        32.0,
        64.0,
        128.0,
        256.0,
        512.0,
        1024.0,
        f64::INFINITY,
    ];
    let mut buckets = bounds
        .iter()
        .map(|b| Bucket {
            upper_limit: *b,
            count: 0,
        })
        .collect::<Vec<_>>();

    let mut sum = 0.0;
    let mut count = 0;
    for value in values {
        for bucket in buckets.iter_mut() {
            if *value <= bucket.upper_limit {
                bucket.count += 1;
            }
        }

        sum += *value;
        count += 1;
    }

    (buckets, sum, count)
}

pub fn generate_f64s(start: u16, end: u16) -> Vec<f64> {
    assert!(start <= end);
    let mut samples = Vec::new();
    for n in start..=end {
        samples.push(f64::from(n));
    }
    samples
}

pub fn get_set<S, V>(values: S, kind: MetricKind) -> Metric
where
    S: IntoIterator<Item = V>,
    V: Display,
{
    Metric::new(
        "set",
        kind,
        MetricValue::Set {
            values: values.into_iter().map(|i| i.to_string()).collect(),
        },
    )
}

pub fn get_distribution<S, V>(samples: S, kind: MetricKind) -> Metric
where
    S: IntoIterator<Item = V>,
    V: Into<f64>,
{
    Metric::new(
        "distribution",
        kind,
        MetricValue::Distribution {
            samples: samples
                .into_iter()
                .map(|n| Sample {
                    value: n.into(),
                    rate: 1,
                })
                .collect(),
            statistic: StatisticKind::Histogram,
        },
    )
}

pub fn get_aggregated_histogram<S, V>(samples: S, kind: MetricKind) -> Metric
where
    S: IntoIterator<Item = V>,
    V: Into<f64>,
{
    let samples = samples.into_iter().map(Into::into).collect::<Vec<_>>();
    let (buckets, sum, count) = buckets_from_samples(&samples);

    Metric::new(
        "agg_histogram",
        kind,
        MetricValue::AggregatedHistogram {
            buckets,
            count,
            sum,
        },
    )
}

pub fn get_counter(value: f64, kind: MetricKind) -> Metric {
    Metric::new("counter", kind, MetricValue::Counter { value })
}

pub fn get_gauge(value: f64, kind: MetricKind) -> Metric {
    Metric::new("gauge", kind, MetricValue::Gauge { value })
}

pub fn assert_normalize<N: MetricNormalize>(
    mut normalizer: N,
    inputs: Vec<Metric>,
    expected_outputs: Vec<Option<Metric>>,
) {
    let mut metric_set = MetricSet::default();

    for (input, expected) in inputs.into_iter().zip(expected_outputs) {
        let result = normalizer.normalize(&mut metric_set, input);
        assert_eq!(result, expected);
    }
}

pub mod tests {
    use super::*;

    pub fn absolute_counter_normalize_to_incremental<N: MetricNormalize>(normalizer: N) {
        let first_value = 3.14;
        let second_value = 8.675309;

        let counters = vec![
            get_counter(first_value, MetricKind::Absolute),
            get_counter(second_value, MetricKind::Absolute),
        ];

        let expected_counters = vec![
            None,
            Some(get_counter(
                second_value - first_value,
                MetricKind::Incremental,
            )),
        ];

        assert_normalize(normalizer, counters, expected_counters);
    }

    pub fn incremental_counter_normalize_to_incremental<N: MetricNormalize>(normalizer: N) {
        let first_value = 3.14;
        let second_value = 8.675309;

        let counters = vec![
            get_counter(first_value, MetricKind::Incremental),
            get_counter(second_value, MetricKind::Incremental),
        ];

        let expected_counters = counters
            .clone()
            .into_iter()
            .map(Option::Some)
            .collect::<Vec<_>>();

        assert_normalize(normalizer, counters, expected_counters);
    }

    pub fn mixed_counter_normalize_to_incremental<N: MetricNormalize>(normalizer: N) {
        let first_value = 3.14;
        let second_value = 8.675309;
        let third_value = 16.19;

        let counters = vec![
            get_counter(first_value, MetricKind::Incremental),
            get_counter(second_value, MetricKind::Absolute),
            get_counter(third_value, MetricKind::Absolute),
            get_counter(first_value, MetricKind::Absolute),
            get_counter(second_value, MetricKind::Incremental),
            get_counter(third_value, MetricKind::Incremental),
        ];

        let expected_counters = vec![
            Some(get_counter(first_value, MetricKind::Incremental)),
            None,
            Some(get_counter(
                third_value - second_value,
                MetricKind::Incremental,
            )),
            None,
            Some(get_counter(second_value, MetricKind::Incremental)),
            Some(get_counter(third_value, MetricKind::Incremental)),
        ];

        assert_normalize(normalizer, counters, expected_counters);
    }

    pub fn absolute_gauge_normalize_to_absolute<N: MetricNormalize>(normalizer: N) {
        let first_value = 3.14;
        let second_value = 8.675309;

        let gauges = vec![
            get_gauge(first_value, MetricKind::Absolute),
            get_gauge(second_value, MetricKind::Absolute),
        ];

        let expected_gauges = gauges
            .clone()
            .into_iter()
            .map(Option::Some)
            .collect::<Vec<_>>();

        assert_normalize(normalizer, gauges, expected_gauges);
    }

    pub fn incremental_gauge_normalize_to_absolute<N: MetricNormalize>(normalizer: N) {
        let first_value = 3.14;
        let second_value = 8.675309;

        let gauges = vec![
            get_gauge(first_value, MetricKind::Incremental),
            get_gauge(second_value, MetricKind::Incremental),
        ];

        let expected_gauges = vec![
            Some(get_gauge(first_value, MetricKind::Absolute)),
            Some(get_gauge(first_value + second_value, MetricKind::Absolute)),
        ];

        assert_normalize(normalizer, gauges, expected_gauges);
    }

    pub fn mixed_gauge_normalize_to_absolute<N: MetricNormalize>(normalizer: N) {
        let first_value = 3.14;
        let second_value = 8.675309;
        let third_value = 16.19;

        let gauges = vec![
            get_gauge(first_value, MetricKind::Incremental),
            get_gauge(second_value, MetricKind::Absolute),
            get_gauge(third_value, MetricKind::Absolute),
            get_gauge(first_value, MetricKind::Absolute),
            get_gauge(second_value, MetricKind::Incremental),
            get_gauge(third_value, MetricKind::Incremental),
        ];

        let expected_gauges = vec![
            Some(get_gauge(first_value, MetricKind::Absolute)),
            Some(get_gauge(second_value, MetricKind::Absolute)),
            Some(get_gauge(third_value, MetricKind::Absolute)),
            Some(get_gauge(first_value, MetricKind::Absolute)),
            Some(get_gauge(first_value + second_value, MetricKind::Absolute)),
            Some(get_gauge(
                first_value + second_value + third_value,
                MetricKind::Absolute,
            )),
        ];

        assert_normalize(normalizer, gauges, expected_gauges);
    }

    pub fn absolute_set_normalize_to_incremental<N: MetricNormalize>(normalizer: N) {
        let sets = vec![
            get_set(1..=20, MetricKind::Absolute),
            get_set(15..=25, MetricKind::Absolute),
        ];

        let expected_sets = vec![None, Some(get_set(21..=25, MetricKind::Incremental))];

        assert_normalize(normalizer, sets, expected_sets);
    }

    pub fn incremental_set_normalize_to_incremental<N: MetricNormalize>(normalizer: N) {
        let sets = vec![
            get_set(1..=20, MetricKind::Incremental),
            get_set(15..=25, MetricKind::Incremental),
        ];

        let expected_sets = vec![
            Some(get_set(1..=20, MetricKind::Incremental)),
            Some(get_set(15..=25, MetricKind::Incremental)),
        ];

        assert_normalize(normalizer, sets, expected_sets);
    }

    pub fn mixed_set_normalize_to_incremental<N: MetricNormalize>(normalizer: N) {
        let sets = vec![
            get_set(1..=20, MetricKind::Incremental),
            get_set(10..=16, MetricKind::Absolute),
            get_set(15..=25, MetricKind::Absolute),
            get_set(1..5, MetricKind::Incremental),
            get_set(3..=42, MetricKind::Incremental),
        ];

        let expected_sets = vec![
            Some(get_set(1..=20, MetricKind::Incremental)),
            None,
            Some(get_set(17..=25, MetricKind::Incremental)),
            Some(get_set(1..5, MetricKind::Incremental)),
            Some(get_set(3..=42, MetricKind::Incremental)),
        ];

        assert_normalize(normalizer, sets, expected_sets);
    }
}
