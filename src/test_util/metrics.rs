use std::collections::{HashMap, HashSet};

use vector_core::event::{
    metric::{MetricData, MetricSeries, Sample},
    Event, EventMetadata, Metric, MetricValue,
};

use crate::sinks::util::buffer::metrics::{MetricNormalize, MetricSet};

type SplitMetrics = HashMap<MetricSeries, (MetricData, EventMetadata)>;

#[derive(Default)]
struct PassthroughNormalizer;

impl MetricNormalize for PassthroughNormalizer {
    fn apply_state(&mut self, state: &mut MetricSet, metric: Metric) -> Option<Metric> {
        state.make_absolute(metric)
    }
}

#[derive(Default)]
pub struct MetricState {
    intermediate: MetricSet,
    normalizer: PassthroughNormalizer,
    latest: HashMap<MetricSeries, (MetricData, EventMetadata)>,
}

impl MetricState {
    pub fn merge(&mut self, metric: Metric) {
        if let Some(output) = self.normalizer.apply_state(&mut self.intermediate, metric) {
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

impl Extend<Event> for MetricState {
    fn extend<T: IntoIterator<Item = Event>>(&mut self, iter: T) {
        for event in iter.into_iter() {
            self.merge(event.into_metric());
        }
    }
}

pub fn read_counter_value(metrics: &SplitMetrics, series: MetricSeries) -> Option<f64> {
    metrics
        .get(&series)
        .map(|(data, _)| match data.value() {
            MetricValue::Counter { value } => Some(*value),
            _ => None,
        })
        .flatten()
}

pub fn read_gauge_value(metrics: &SplitMetrics, series: MetricSeries) -> Option<f64> {
    metrics
        .get(&series)
        .map(|(data, _)| match data.value() {
            MetricValue::Gauge { value } => Some(*value),
            _ => None,
        })
        .flatten()
}

pub fn read_distribution_samples(
    metrics: &SplitMetrics,
    series: MetricSeries,
) -> Option<Vec<Sample>> {
    metrics
        .get(&series)
        .map(|(data, _)| match data.value() {
            MetricValue::Distribution { samples, .. } => Some(samples.clone()),
            _ => None,
        })
        .flatten()
}

pub fn read_set_values(metrics: &SplitMetrics, series: MetricSeries) -> Option<HashSet<String>> {
    metrics
        .get(&series)
        .map(|(data, _)| match data.value() {
            MetricValue::Set { values } => Some(values.iter().cloned().collect()),
            _ => None,
        })
        .flatten()
}

macro_rules! series {
	($name:expr) => {
		vector_core::event::metric::MetricSeries {
			name: vector_core::event::metric::MetricName {
				name: $name.into(),
				namespace: None,
			},
			tags: None,
		}
	};
	($name:expr, $($tk:expr => $tv:expr),*) => {
		vector_core::event::metric::MetricSeries {
			name: vector_core::event::metric::MetricName {
				name: $name.into(),
				namespace: None,
			},
			tags: Some(vector_core::event::metric::MetricTags::from_iter(
				vec![$(($tk.into(), $tv.into())),*]
			)),
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

pub(crate) use series;
