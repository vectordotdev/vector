//! Incremental Prometheus histogram normalization with bounded per-series state.
use std::{
    collections::{HashMap, hash_map::Entry},
    time::Instant,
};

use vector_lib::event::metric::{
    Bucket, Metric, MetricKind, MetricSeries, MetricValue, StatisticKind,
};

use crate::sinks::util::buffer::metrics::{MetricNormalize, MetricSet};

struct RunningHistogram {
    buckets: Vec<Bucket>,
    count: u64,
    sum: f64,
    updated_at: Instant,
}

impl RunningHistogram {
    fn new(bounds: &[f64]) -> Self {
        Self {
            buckets: bounds
                .iter()
                .map(|limit| Bucket {
                    upper_limit: *limit,
                    count: 0,
                })
                .collect(),
            count: 0,
            sum: 0.0,
            updated_at: Instant::now(),
        }
    }

    fn add_samples(&mut self, samples: &[vector_lib::event::metric::Sample]) {
        for sample in samples {
            let rate = u64::from(sample.rate);
            if let Some(bucket) = self
                .buckets
                .iter_mut()
                .find(|bucket| bucket.upper_limit >= sample.value)
            {
                bucket.count += rate;
            }
            // Keep the precise operation order of samples_to_buckets.
            self.sum += sample.value * f64::from(sample.rate);
            self.count += rate;
        }
        self.updated_at = Instant::now();
    }

    fn value(&self) -> MetricValue {
        MetricValue::AggregatedHistogram {
            buckets: self.buckets.clone(),
            count: self.count,
            sum: Some(self.sum),
        }
    }
}

pub struct PrometheusMetricNormalize {
    buckets: Vec<f64>,
    finite_buckets: bool,
    fast_histograms: HashMap<MetricSeries, RunningHistogram>,
    last_cleanup: Instant,
}

impl PrometheusMetricNormalize {
    pub(super) fn new(buckets: Vec<f64>) -> Self {
        let finite_buckets = buckets.iter().all(|bound| bound.is_finite());
        Self {
            buckets,
            finite_buckets,
            fast_histograms: HashMap::new(),
            last_cleanup: Instant::now(),
        }
    }

    fn cleanup(&mut self, state: &MetricSet) {
        let Some(policy) = state.ttl_policy() else {
            return;
        };
        let now = Instant::now();
        if now.duration_since(self.last_cleanup) >= policy.cleanup_interval {
            self.fast_histograms
                .retain(|_, histogram| now.duration_since(histogram.updated_at) < policy.ttl);
            self.last_cleanup = now;
        }
    }
}

impl MetricNormalize for PrometheusMetricNormalize {
    fn normalize(&mut self, state: &mut MetricSet, metric: Metric) -> Option<Metric> {
        self.cleanup(state);

        if metric.kind() != MetricKind::Incremental {
            // Absolute metrics do not replace the incremental cache in MetricSet.
            return state.make_absolute(metric);
        }

        if self.finite_buckets
            && let MetricValue::Distribution {
                samples,
                statistic: StatisticKind::Histogram,
            } = metric.value()
        {
            let series = metric.series().clone();
            let histogram = match self.fast_histograms.entry(series) {
                Entry::Occupied(entry) => entry.into_mut(),
                Entry::Vacant(entry) => {
                    // The original normalizer resets when the input representation changes.
                    state.remove(metric.series());
                    entry.insert(RunningHistogram::new(&self.buckets))
                }
            };
            histogram.add_samples(samples);
            // The fast path bypasses MetricSet::make_absolute; keep its TTL cleanup active.
            state.maybe_cleanup();
            return Some(metric.with_value(histogram.value()).into_absolute());
        }

        if self.fast_histograms.remove(metric.series()).is_some() {
            state.remove(metric.series());
        }
        // The existing path retains its behavior for counters, summaries,
        // source-aggregated histograms, and non-finite bucket configurations.
        state.make_absolute(metric)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{thread, time::Duration};

    use vector_lib::event::metric::Sample;

    use crate::sinks::{
        prometheus::collector::{MetricCollector, TimeSeries},
        util::buffer::metrics::TtlPolicy,
    };

    fn histogram(name: &str, values: &[(f64, u32)]) -> Metric {
        Metric::new(
            name,
            MetricKind::Incremental,
            MetricValue::Distribution {
                samples: values
                    .iter()
                    .map(|(value, rate)| Sample {
                        value: *value,
                        rate: *rate,
                    })
                    .collect(),
                statistic: StatisticKind::Histogram,
            },
        )
        .with_timestamp(Some(
            chrono::DateTime::<chrono::Utc>::from_timestamp(1_700_000_000, 0).unwrap(),
        ))
    }

    fn assert_wire_eq(expected: &Metric, actual: &Metric, bounds: &[f64]) {
        let mut reference = TimeSeries::new();
        reference.encode_metric(None, bounds, &[], expected);
        let mut candidate = TimeSeries::new();
        candidate.encode_metric(None, bounds, &[], actual);
        let expected = reference.finish();
        let actual = candidate.finish();
        assert_eq!(actual.timeseries, expected.timeseries);
        assert_eq!(actual.metadata, expected.metadata);
    }

    #[test]
    fn hot_histogram_matches_legacy_across_256_updates() {
        let bounds = vec![-1.0, 0.0, 0.5, 1.0];
        let mut reference = MetricSet::default();
        let mut cache = MetricSet::default();
        let mut normalizer = PrometheusMetricNormalize::new(bounds.clone());
        let values = [-1e16, 1e16, 1.0, 0.25, -0.75, 0.0, 3.0];

        for index in 0..256_usize {
            let metric = histogram(
                "hot",
                &[
                    (
                        values[index % values.len()],
                        u32::try_from(index % 4).unwrap(),
                    ),
                    (values[(index + 1) % values.len()], 1),
                ],
            );
            let expected = reference.make_absolute(metric.clone()).unwrap();
            let actual = normalizer.normalize(&mut cache, metric).unwrap();
            if (index + 1).is_power_of_two() {
                assert_wire_eq(&expected, &actual, &bounds);
            }
        }
        assert_eq!(normalizer.fast_histograms.len(), 1);
        assert_eq!(
            normalizer
                .fast_histograms
                .values()
                .next()
                .unwrap()
                .buckets
                .len(),
            bounds.len()
        );
        assert!(cache.is_empty());
    }

    #[test]
    fn representation_changes_reset_histogram_accumulation() {
        let bounds = vec![0.0, 1.0];
        let mut reference = MetricSet::default();
        let mut state = MetricSet::default();
        let mut normalizer = PrometheusMetricNormalize::new(bounds.clone());
        let aggregate = Metric::new(
            "mixed",
            MetricKind::Incremental,
            MetricValue::AggregatedHistogram {
                buckets: vec![
                    Bucket {
                        upper_limit: 0.0,
                        count: 0,
                    },
                    Bucket {
                        upper_limit: 1.0,
                        count: 2,
                    },
                ],
                count: 2,
                sum: None,
            },
        )
        .with_timestamp(Some(
            chrono::DateTime::<chrono::Utc>::from_timestamp(1_700_000_000, 0).unwrap(),
        ));
        for metric in [
            histogram("mixed", &[(0.25, 1)]),
            aggregate,
            histogram("mixed", &[(0.5, 2)]),
        ] {
            let expected = reference.make_absolute(metric.clone()).unwrap();
            let actual = normalizer.normalize(&mut state, metric).unwrap();
            assert_wire_eq(&expected, &actual, &bounds);
        }
    }

    #[test]
    fn ttl_reaps_both_optimized_and_legacy_state() {
        let mut ttl = TtlPolicy::new(Duration::from_millis(5));
        ttl.cleanup_interval = Duration::from_millis(1);
        let mut cache = MetricSet::with_policies(None, Some(ttl));
        let mut normalizer = PrometheusMetricNormalize::new(vec![0.5]);
        normalizer
            .normalize(
                &mut cache,
                Metric::new(
                    "stale_counter",
                    MetricKind::Incremental,
                    MetricValue::Counter { value: 1.0 },
                ),
            )
            .unwrap();
        normalizer
            .normalize(&mut cache, histogram("hot", &[(0.25, 1)]))
            .unwrap();
        assert_eq!(cache.len(), 1);
        thread::sleep(Duration::from_millis(30));
        let result = normalizer
            .normalize(&mut cache, histogram("hot", &[(0.25, 1)]))
            .unwrap();
        assert!(matches!(
            result.value(),
            MetricValue::AggregatedHistogram { count: 1, .. }
        ));
        assert!(cache.is_empty());
    }

    #[test]
    fn non_finite_bounds_fall_back_to_legacy() {
        for bounds in [
            vec![f64::NAN, 0.5],
            vec![f64::NEG_INFINITY, 0.5],
            vec![0.5, f64::INFINITY],
        ] {
            let mut reference = MetricSet::default();
            let mut cache = MetricSet::default();
            let mut normalizer = PrometheusMetricNormalize::new(bounds.clone());
            for _ in 0..3 {
                let metric = histogram("special", &[(0.25, 1)]);
                let expected = reference.make_absolute(metric.clone()).unwrap();
                let actual = normalizer.normalize(&mut cache, metric).unwrap();
                assert_wire_eq(&expected, &actual, &bounds);
            }
            assert!(normalizer.fast_histograms.is_empty());
        }
    }

    #[test]
    fn event_finalizers_remain_on_output_not_cached_state() {
        let mut cache = MetricSet::default();
        let mut normalizer = PrometheusMetricNormalize::new(vec![0.5]);
        let (notifier, _receiver) = vector_lib::event::BatchNotifier::new_with_receiver();
        for _ in 0..2 {
            let metric = histogram("finalized", &[(0.25, 1)]).with_batch_notifier(&notifier);
            let output = normalizer.normalize(&mut cache, metric).unwrap();
            assert_eq!(output.metadata().finalizers().len(), 1);
        }
        assert!(cache.is_empty());
        assert_eq!(normalizer.fast_histograms.len(), 1);
    }
}
