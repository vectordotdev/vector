//! Converts incremental metrics into absolute ones for the VictoriaMetrics sink.
//!
//! Counters, gauges, sets, aggregated histograms, aggregated summaries, and sketches use the shared
//! [`MetricSet`]. Distributions do not: `MetricValue::add` concatenates distribution samples, so a
//! long-lived incremental distribution grows without bound. Instead, every incoming distribution
//! is recorded into a sink-owned [`VmHistogram`] per series, which holds at most 488 counters.

use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use vector_lib::event::{
    Metric, MetricKind, MetricValue,
    metric::{MetricSeries, Sample},
};

use super::histogram::VmHistogram;
use crate::sinks::util::buffer::metrics::{MetricSet, TtlPolicy};

/// An absolute metric ready to be encoded.
pub(super) struct NormalizedMetric {
    pub(super) metric: Metric,
    /// The cumulative histogram of a distribution metric.
    pub(super) histogram: Option<VmHistogram>,
}

pub(super) struct VmNormalizer {
    metrics: MetricSet,
    histograms: HistogramSet,
}

impl VmNormalizer {
    pub(super) fn new(ttl: Option<Duration>) -> Self {
        Self {
            metrics: MetricSet::with_policies(None, ttl.map(TtlPolicy::new)),
            histograms: HistogramSet::new(ttl),
        }
    }

    pub(super) fn normalize(&mut self, metric: Metric) -> Option<NormalizedMetric> {
        let histogram = match metric.value() {
            MetricValue::Distribution { samples, .. } => Some(match metric.kind() {
                MetricKind::Absolute => VmHistogram::from_samples(samples),
                MetricKind::Incremental => self.histograms.record(metric.series(), samples),
            }),
            _ => None,
        };

        match histogram {
            Some(histogram) => Some(NormalizedMetric {
                metric: metric.into_absolute(),
                histogram: Some(histogram),
            }),
            None => self
                .metrics
                .make_absolute(metric)
                .map(|metric| NormalizedMetric {
                    metric,
                    histogram: None,
                }),
        }
    }
}

/// Cumulative histograms of incremental distributions, keyed by series.
struct HistogramSet {
    series: HashMap<MetricSeries, HistogramEntry>,
    ttl: Option<Duration>,
    last_cleanup: Instant,
}

struct HistogramEntry {
    histogram: VmHistogram,
    last_seen: Instant,
}

impl HistogramSet {
    fn new(ttl: Option<Duration>) -> Self {
        Self {
            series: HashMap::new(),
            ttl,
            last_cleanup: Instant::now(),
        }
    }

    /// Records `samples` into the histogram of `series` and returns the updated histogram.
    fn record(&mut self, series: &MetricSeries, samples: &[Sample]) -> VmHistogram {
        let now = Instant::now();
        self.cleanup(now);

        if let Some(entry) = self.series.get_mut(series) {
            entry.histogram.record_samples(samples);
            entry.last_seen = now;
            return entry.histogram.clone();
        }

        let histogram = VmHistogram::from_samples(samples);
        self.series.insert(
            series.clone(),
            HistogramEntry {
                histogram: histogram.clone(),
                last_seen: now,
            },
        );
        histogram
    }

    /// Drops series not updated within the TTL. Runs at most once per TTL period.
    fn cleanup(&mut self, now: Instant) {
        let Some(ttl) = self.ttl else {
            return;
        };
        if now.duration_since(self.last_cleanup) < ttl {
            return;
        }
        self.series
            .retain(|_, entry| now.duration_since(entry.last_seen) < ttl);
        self.last_cleanup = now;
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.series.len()
    }
}

#[cfg(test)]
mod tests {
    use vector_lib::event::{StatisticKind, metric::Sample};

    use super::*;

    fn distribution(kind: MetricKind, values: &[f64]) -> Metric {
        Metric::new(
            "latency",
            kind,
            MetricValue::Distribution {
                samples: values
                    .iter()
                    .map(|value| Sample {
                        value: *value,
                        rate: 1,
                    })
                    .collect(),
                statistic: StatisticKind::Histogram,
            },
        )
    }

    fn counter(kind: MetricKind, value: f64) -> Metric {
        Metric::new("requests", kind, MetricValue::Counter { value })
    }

    #[test]
    fn accumulates_incremental_counters() {
        let mut normalizer = VmNormalizer::new(None);

        normalizer.normalize(counter(MetricKind::Incremental, 1.0));
        let output = normalizer
            .normalize(counter(MetricKind::Incremental, 2.0))
            .unwrap();

        assert_eq!(output.metric.kind(), MetricKind::Absolute);
        assert_eq!(output.metric.value(), &MetricValue::Counter { value: 3.0 });
        assert!(output.histogram.is_none());
    }

    #[test]
    fn accumulates_incremental_distributions() {
        let mut normalizer = VmNormalizer::new(None);

        normalizer.normalize(distribution(MetricKind::Incremental, &[0.5, 2.0]));
        let output = normalizer
            .normalize(distribution(MetricKind::Incremental, &[0.5]))
            .unwrap();

        assert_eq!(output.metric.kind(), MetricKind::Absolute);
        let histogram = output.histogram.unwrap();
        assert_eq!(histogram.count(), 3);
        assert_eq!(histogram.sum(), 3.0);
        assert_eq!(
            histogram.buckets().collect::<Vec<_>>(),
            vec![("4.642e-01...5.275e-01", 2), ("1.896e+00...2.154e+00", 1)]
        );
    }

    #[test]
    fn incremental_distribution_state_is_bounded() {
        let mut normalizer = VmNormalizer::new(None);

        for _ in 0..1_000 {
            normalizer.normalize(distribution(MetricKind::Incremental, &[0.5, 2.0, 7.0]));
        }

        assert_eq!(normalizer.histograms.len(), 1);
        let histogram = &normalizer
            .histograms
            .series
            .values()
            .next()
            .unwrap()
            .histogram;
        assert_eq!(histogram.count(), 3_000);
        assert_eq!(histogram.bucket_len(), 3);
    }

    #[test]
    fn absolute_distributions_are_not_accumulated() {
        let mut normalizer = VmNormalizer::new(None);

        normalizer.normalize(distribution(MetricKind::Absolute, &[0.5]));
        let output = normalizer
            .normalize(distribution(MetricKind::Absolute, &[0.5]))
            .unwrap();

        assert_eq!(output.histogram.unwrap().count(), 1);
        assert_eq!(normalizer.histograms.len(), 0);
    }

    #[test]
    fn expires_distribution_state() {
        let ttl = Duration::from_secs(60);
        let mut set = HistogramSet::new(Some(ttl));
        let series = distribution(MetricKind::Incremental, &[]).series().clone();

        set.record(
            &series,
            &[Sample {
                value: 1.0,
                rate: 1,
            }],
        );
        assert_eq!(set.len(), 1);

        set.cleanup(Instant::now() + ttl * 2);
        assert_eq!(set.len(), 0);
    }
}
