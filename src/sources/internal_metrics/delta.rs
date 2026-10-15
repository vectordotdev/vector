use std::collections::HashMap;

use vector_lib::{
    event::{Metric, MetricKind, MetricValue, metric::MetricSeries},
    metrics::CARDINALITY_COUNTER_KEY_NAME,
};

/// Previous absolute value of a series, stamped with the scrape that last observed it.
struct Entry {
    value: MetricValue,
    generation: u64,
}

/// Converts the absolute values captured from the metrics registry into increments.
///
/// Registry handles are created at zero within the process, so the first value observed for a
/// series *is* its increment. That differs from externally scraped metrics, where sinks must
/// discard the first observation to avoid emitting an accumulated counter as one huge delta.
#[derive(Default)]
pub(super) struct DeltaState {
    seen: HashMap<MetricSeries, Entry>,
    generation: u64,
}

impl DeltaState {
    /// Records the given metrics as the baseline without emitting anything.
    pub(super) fn seed(&mut self, mut metrics: Vec<Metric>) {
        self.convert(&mut metrics);
    }

    /// Rewrites counters and histograms in place into increments over the previous scrape.
    ///
    /// Gauges are already meaningful as absolute values and pass through untouched.
    pub(super) fn convert(&mut self, metrics: &mut [Metric]) {
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;

        for metric in metrics {
            if !is_cumulative(metric) {
                continue;
            }

            let absolute = metric.value().clone();
            match self.seen.get_mut(metric.series()) {
                Some(entry) => {
                    if !metric.value_mut().subtract(&entry.value) {
                        // `subtract` leaves the value untouched when it detects a reset, so it is
                        // already the increment from a series that restarted at zero.
                        debug!(
                            message = "Internal metric series reset, reporting its full value.",
                            series = ?metric.series(),
                        );
                    }
                    entry.value = absolute;
                    entry.generation = generation;
                }
                None => {
                    self.seen.insert(
                        metric.series().clone(),
                        Entry {
                            value: absolute,
                            generation,
                        },
                    );
                }
            }
            metric.data_mut().kind = MetricKind::Incremental;
        }

        // Forget series that expired from the registry, bounding the map.
        self.seen.retain(|_, entry| entry.generation == generation);
    }
}

/// Whether a captured metric accumulates over the process lifetime and so needs differencing.
fn is_cumulative(metric: &Metric) -> bool {
    // The cardinality counter is declared a counter but reports the current series count, so it is
    // not monotonic and has to stay absolute.
    if metric.name() == CARDINALITY_COUNTER_KEY_NAME {
        return false;
    }
    matches!(
        metric.value(),
        MetricValue::Counter { .. } | MetricValue::AggregatedHistogram { .. }
    )
}

#[cfg(test)]
mod tests {
    use vector_lib::event::metric::Bucket;

    use super::*;

    fn counter(name: &str, value: f64) -> Metric {
        Metric::new(name, MetricKind::Absolute, MetricValue::Counter { value })
    }

    fn histogram(count: u64, sum: f64) -> Metric {
        Metric::new(
            "histo",
            MetricKind::Absolute,
            MetricValue::AggregatedHistogram {
                buckets: vec![Bucket {
                    upper_limit: 1.0,
                    count,
                }],
                count,
                sum,
            },
        )
    }

    fn convert_one(state: &mut DeltaState, metric: Metric) -> Metric {
        let mut metrics = [metric];
        state.convert(&mut metrics);
        let [metric] = metrics;
        metric
    }

    #[test]
    fn first_sighting_emits_full_value_as_increment() {
        let mut state = DeltaState::default();
        let metric = convert_one(&mut state, counter("a", 7.0));

        assert_eq!(metric.kind(), MetricKind::Incremental);
        assert_eq!(metric.value(), &MetricValue::Counter { value: 7.0 });
    }

    #[test]
    fn subsequent_sightings_emit_the_delta() {
        let mut state = DeltaState::default();
        convert_one(&mut state, counter("a", 7.0));

        let metric = convert_one(&mut state, counter("a", 10.0));
        assert_eq!(metric.value(), &MetricValue::Counter { value: 3.0 });

        let metric = convert_one(&mut state, counter("a", 10.5));
        assert_eq!(metric.value(), &MetricValue::Counter { value: 0.5 });
    }

    #[test]
    fn idle_counter_emits_zero_delta() {
        let mut state = DeltaState::default();
        convert_one(&mut state, counter("a", 7.0));

        let metric = convert_one(&mut state, counter("a", 7.0));
        assert_eq!(metric.value(), &MetricValue::Counter { value: 0.0 });
    }

    #[test]
    fn counter_reset_emits_full_value() {
        let mut state = DeltaState::default();
        convert_one(&mut state, counter("a", 100.0));

        // The registry restarted this series from zero, so 2.0 is the whole increment.
        let metric = convert_one(&mut state, counter("a", 2.0));
        assert_eq!(metric.value(), &MetricValue::Counter { value: 2.0 });

        // The reset value became the new baseline.
        let metric = convert_one(&mut state, counter("a", 5.0));
        assert_eq!(metric.value(), &MetricValue::Counter { value: 3.0 });
    }

    #[test]
    fn histogram_buckets_are_differenced() {
        let mut state = DeltaState::default();
        convert_one(&mut state, histogram(2, 11.0));

        let metric = convert_one(&mut state, histogram(5, 30.0));
        assert_eq!(metric.kind(), MetricKind::Incremental);
        assert_eq!(
            metric.value(),
            &MetricValue::AggregatedHistogram {
                buckets: vec![Bucket {
                    upper_limit: 1.0,
                    count: 3
                }],
                count: 3,
                sum: 19.0,
            }
        );
    }

    #[test]
    fn gauges_pass_through_as_absolute() {
        let mut state = DeltaState::default();
        let gauge = Metric::new("g", MetricKind::Absolute, MetricValue::Gauge { value: 2.0 });

        let metric = convert_one(&mut state, gauge.clone());
        assert_eq!(metric.kind(), MetricKind::Absolute);
        assert_eq!(metric.value(), &MetricValue::Gauge { value: 2.0 });

        // No state is retained, so a later lower value is still reported verbatim.
        let metric = convert_one(&mut state, gauge);
        assert_eq!(metric.value(), &MetricValue::Gauge { value: 2.0 });
    }

    #[test]
    fn cardinality_counter_passes_through_as_absolute() {
        let mut state = DeltaState::default();
        convert_one(&mut state, counter(CARDINALITY_COUNTER_KEY_NAME, 10.0));

        let metric = convert_one(&mut state, counter(CARDINALITY_COUNTER_KEY_NAME, 4.0));
        assert_eq!(metric.kind(), MetricKind::Absolute);
        assert_eq!(metric.value(), &MetricValue::Counter { value: 4.0 });
    }

    #[test]
    fn seed_suppresses_the_baseline() {
        let mut state = DeltaState::default();
        state.seed(vec![counter("a", 100.0)]);

        let metric = convert_one(&mut state, counter("a", 103.0));
        assert_eq!(metric.value(), &MetricValue::Counter { value: 3.0 });
    }

    #[test]
    fn expired_series_are_forgotten() {
        let mut state = DeltaState::default();
        convert_one(&mut state, counter("a", 100.0));
        assert_eq!(state.seen.len(), 1);

        // A scrape without `a` means the registry expired it.
        convert_one(&mut state, counter("b", 1.0));
        assert_eq!(state.seen.len(), 1);

        // So `a` re-registering counts from zero again.
        let metric = convert_one(&mut state, counter("a", 4.0));
        assert_eq!(metric.value(), &MetricValue::Counter { value: 4.0 });
    }
}
