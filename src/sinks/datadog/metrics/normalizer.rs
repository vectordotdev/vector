use vector_lib::{
    event::{Metric, MetricValue},
    metrics::AgentDDSketch,
};

use crate::sinks::util::buffer::metrics::{MetricNormalize, MetricSet};

#[derive(Default)]
pub(crate) struct DatadogMetricsNormalizer;

impl MetricNormalize for DatadogMetricsNormalizer {
    fn normalize(&mut self, state: &mut MetricSet, metric: Metric) -> Option<Metric> {
        // The periodic TTL sweep (for `expire_metrics_secs`) is normally driven by
        // `MetricSet::make_absolute`/`make_incremental`, called below for every branch except
        // `MetricValue::Sketch`, which is passed through without touching `state` at all. Without
        // this, a sink that settles into sketch-only traffic would never sweep entries left behind
        // by earlier non-sketch series, leaking them for as long as the process runs despite a
        // configured TTL. Driving it unconditionally here covers that gap; it's cheap to call
        // redundantly for the other branches since `maybe_cleanup` no-ops unless the cleanup
        // interval has actually elapsed.
        state.maybe_cleanup();

        // We primarily care about making sure that counters are incremental, and that gauges are
        // always absolute.  For other metric kinds, we want them to be incremental.
        match &metric.value() {
            // We always send counters as incremental and gauges as absolute.  Realistically, any
            // system sending an incremental gauge update is kind of doing it wrong, but alas.
            MetricValue::Gauge { .. } => state.make_absolute(metric),
            // We convert distributions and aggregated histograms to sketches internally. We can't
            // send absolute sketches to Datadog, though, so we incrementalize them first.
            MetricValue::Distribution { .. } | MetricValue::AggregatedHistogram { .. } => state
                .make_incremental(metric)
                .filter(|metric| !metric.value().is_empty())
                .and_then(|metric| AgentDDSketch::transform_to_sketch(metric).ok()),
            // Sketches cannot be subtracted from one another, so we treat them as implicitly
            // incremental, and just update the metric type.
            MetricValue::Sketch { .. } => Some(metric.into_incremental()),
            // Otherwise, ensure that it's incremental.
            _ => state.make_incremental(metric),
        }
    }
}

#[cfg(test)]
mod tests {
    use vector_lib::{
        event::{Metric, MetricKind, MetricValue, metric::MetricSketch},
        metrics::AgentDDSketch,
    };

    use super::DatadogMetricsNormalizer;
    use crate::{
        sinks::util::buffer::metrics::{MetricNormalize, MetricSet, TtlPolicy},
        test_util::metrics::{
            assert_normalize, generate_f64s, get_aggregated_histogram, get_distribution, tests,
        },
    };

    // Regression test for a Codex-flagged gap: `MetricValue::Sketch` is passed through directly
    // in `normalize` without ever touching `state` via `make_absolute`/`make_incremental`, which
    // are the only places that used to drive `MetricSet`'s periodic TTL sweep. A sink that
    // receives a non-sketch metric and then settles into sketch-only traffic would therefore
    // never sweep the stale entry left behind, leaking it for as long as the process runs despite
    // a configured `expire_metrics_secs`. `normalize` now calls `state.maybe_cleanup()`
    // unconditionally to cover every branch, including this one.
    #[test]
    fn ttl_sweep_runs_during_sketch_only_traffic() {
        use std::time::{Duration, Instant};

        let ttl = Duration::from_millis(50);
        // Construct the policy directly, rather than via `TtlPolicy::new`, so `cleanup_interval`
        // isn't clamped to its usual 10-second floor: that floor exists to bound real-world sweep
        // overhead, but would make this test wait 10+ seconds for no benefit.
        let policy = TtlPolicy {
            ttl,
            cleanup_interval: Duration::from_millis(10),
            last_cleanup: Instant::now(),
        };
        let mut state = MetricSet::with_policies(None, Some(policy));
        let mut normalizer = DatadogMetricsNormalizer;

        // Seed a stale-to-be gauge entry. `MetricValue::Gauge` goes through `make_absolute`,
        // which only caches the entry for `MetricKind::Incremental` input (via
        // `incremental_to_absolute`'s unconditional `insert`) — an already-`Absolute` gauge is
        // passed straight through without being cached at all.
        let gauge = get_gauge("stale_gauge", 1.0, MetricKind::Incremental);
        assert_eq!(
            normalizer.normalize(&mut state, gauge),
            Some(get_gauge("stale_gauge", 1.0, MetricKind::Absolute))
        );
        assert_eq!(state.len(), 1);

        // Let the gauge entry exceed its TTL, and the (artificially short) cleanup interval
        // elapse.
        std::thread::sleep(ttl + Duration::from_millis(50));

        // Send only a sketch from here on. Sketches never touch `state` directly, so if cleanup
        // weren't driven unconditionally, the stale gauge entry above would never be swept.
        let sketch = get_sketch("some_sketch", [1.0, 2.0], MetricKind::Incremental);
        let _ = normalizer.normalize(&mut state, sketch);

        assert_eq!(
            state.len(),
            0,
            "stale gauge entry should have been swept by the TTL cleanup driven for sketch-only traffic"
        );
    }

    fn get_gauge<N: Into<String>>(name: N, value: f64, kind: MetricKind) -> Metric {
        Metric::new(name, kind, MetricValue::Gauge { value })
    }

    fn get_sketch<N, S, V>(name: N, samples: S, kind: MetricKind) -> Metric
    where
        N: Into<String>,
        S: IntoIterator<Item = V>,
        V: Into<f64>,
    {
        let samples = samples.into_iter().map(Into::into).collect::<Vec<_>>();
        let mut ddsketch = AgentDDSketch::with_agent_defaults();
        ddsketch.insert_many(&samples);

        Metric::new(
            name,
            kind,
            MetricValue::Sketch {
                sketch: MetricSketch::AgentDDSketch(ddsketch),
            },
        )
    }

    #[test]
    fn absolute_counter() {
        tests::absolute_counter_normalize_to_incremental(DatadogMetricsNormalizer);
    }

    #[test]
    fn incremental_counter() {
        tests::incremental_counter_normalize_to_incremental(DatadogMetricsNormalizer);
    }

    #[test]
    fn mixed_counter() {
        tests::mixed_counter_normalize_to_incremental(DatadogMetricsNormalizer);
    }

    #[test]
    fn absolute_gauge() {
        tests::absolute_gauge_normalize_to_absolute(DatadogMetricsNormalizer);
    }

    #[test]
    fn incremental_gauge() {
        tests::incremental_gauge_normalize_to_absolute(DatadogMetricsNormalizer);
    }

    #[test]
    fn mixed_gauge() {
        tests::mixed_gauge_normalize_to_absolute(DatadogMetricsNormalizer);
    }

    #[test]
    fn absolute_set() {
        tests::absolute_set_normalize_to_incremental(DatadogMetricsNormalizer);
    }

    #[test]
    fn incremental_set() {
        tests::incremental_set_normalize_to_incremental(DatadogMetricsNormalizer);
    }

    #[test]
    fn mixed_set() {
        tests::mixed_set_normalize_to_incremental(DatadogMetricsNormalizer);
    }

    #[test]
    fn absolute_distribution() {
        let samples1 = generate_f64s(1, 100);

        let mut samples2 = samples1.clone();
        samples2.extend(generate_f64s(75, 125));

        let sketch_samples = generate_f64s(101, 125);

        let distributions = vec![
            get_distribution(samples1, MetricKind::Absolute),
            get_distribution(samples2, MetricKind::Absolute),
        ];

        let expected_sketches = vec![
            None,
            Some(get_sketch(
                distributions[1].name(),
                sketch_samples,
                MetricKind::Incremental,
            )),
        ];

        assert_normalize(DatadogMetricsNormalizer, distributions, expected_sketches);
    }

    #[test]
    fn incremental_distribution() {
        let samples1 = generate_f64s(1, 100);
        let samples2 = generate_f64s(75, 125);
        let sketch1_samples = samples1.clone();
        let sketch2_samples = samples2.clone();

        let distributions = vec![
            get_distribution(samples1, MetricKind::Incremental),
            get_distribution(samples2, MetricKind::Incremental),
        ];

        let expected_sketches = vec![
            Some(get_sketch(
                distributions[0].name(),
                sketch1_samples,
                MetricKind::Incremental,
            )),
            Some(get_sketch(
                distributions[1].name(),
                sketch2_samples,
                MetricKind::Incremental,
            )),
        ];

        assert_normalize(DatadogMetricsNormalizer, distributions, expected_sketches);
    }

    #[test]
    fn mixed_distribution() {
        let samples1 = generate_f64s(1, 100);
        let samples2 = generate_f64s(75, 125);
        let samples3 = generate_f64s(75, 187);
        let samples4 = generate_f64s(22, 45);
        let samples5 = generate_f64s(1, 100);
        let sketch1_samples = samples1.clone();
        let sketch3_samples = generate_f64s(126, 187);
        let sketch4_samples = samples4.clone();
        let sketch5_samples = samples5.clone();

        let distributions = vec![
            get_distribution(samples1, MetricKind::Incremental),
            get_distribution(samples2, MetricKind::Absolute),
            get_distribution(samples3, MetricKind::Absolute),
            get_distribution(samples4, MetricKind::Incremental),
            get_distribution(samples5, MetricKind::Incremental),
        ];

        let expected_sketches = vec![
            Some(get_sketch(
                distributions[0].name(),
                sketch1_samples,
                MetricKind::Incremental,
            )),
            None,
            Some(get_sketch(
                distributions[2].name(),
                sketch3_samples,
                MetricKind::Incremental,
            )),
            Some(get_sketch(
                distributions[3].name(),
                sketch4_samples,
                MetricKind::Incremental,
            )),
            Some(get_sketch(
                distributions[4].name(),
                sketch5_samples,
                MetricKind::Incremental,
            )),
        ];

        assert_normalize(DatadogMetricsNormalizer, distributions, expected_sketches);
    }

    #[test]
    fn absolute_aggregated_histogram() {
        let samples1 = generate_f64s(1, 100);
        let samples2 = generate_f64s(1, 125);
        let sketch_samples = generate_f64s(101, 125);

        let agg_histograms = vec![
            get_aggregated_histogram(samples1, MetricKind::Absolute),
            get_aggregated_histogram(samples2, MetricKind::Absolute),
        ];

        let expected_sketches = vec![
            None,
            Some(
                AgentDDSketch::transform_to_sketch(get_aggregated_histogram(
                    sketch_samples,
                    MetricKind::Incremental,
                ))
                .unwrap(),
            ),
        ];

        assert_normalize(DatadogMetricsNormalizer, agg_histograms, expected_sketches);
    }

    #[test]
    fn incremental_aggregated_histogram() {
        let samples1 = generate_f64s(1, 100);
        let samples2 = generate_f64s(1, 125);
        let sketch1_samples = samples1.clone();
        let sketch2_samples = samples2.clone();

        let agg_histograms = vec![
            get_aggregated_histogram(samples1, MetricKind::Incremental),
            get_aggregated_histogram(samples2, MetricKind::Incremental),
        ];

        let expected_sketches = vec![
            Some(
                AgentDDSketch::transform_to_sketch(get_aggregated_histogram(
                    sketch1_samples,
                    MetricKind::Incremental,
                ))
                .unwrap(),
            ),
            Some(
                AgentDDSketch::transform_to_sketch(get_aggregated_histogram(
                    sketch2_samples,
                    MetricKind::Incremental,
                ))
                .unwrap(),
            ),
        ];

        assert_normalize(DatadogMetricsNormalizer, agg_histograms, expected_sketches);
    }

    #[test]
    fn mixed_aggregated_histogram() {
        let samples1 = generate_f64s(1, 100);
        let samples2 = generate_f64s(75, 125);
        let samples3 = generate_f64s(75, 187);
        let samples4 = generate_f64s(22, 45);
        let samples5 = generate_f64s(1, 100);
        let sketch1_samples = samples1.clone();
        let sketch3_samples = generate_f64s(126, 187);
        let sketch4_samples = samples4.clone();
        let sketch5_samples = samples5.clone();

        let agg_histograms = vec![
            get_aggregated_histogram(samples1, MetricKind::Incremental),
            get_aggregated_histogram(samples2, MetricKind::Absolute),
            get_aggregated_histogram(samples3, MetricKind::Absolute),
            get_aggregated_histogram(samples4, MetricKind::Incremental),
            get_aggregated_histogram(samples5, MetricKind::Incremental),
        ];

        let expected_sketches = vec![
            Some(
                AgentDDSketch::transform_to_sketch(get_aggregated_histogram(
                    sketch1_samples,
                    MetricKind::Incremental,
                ))
                .unwrap(),
            ),
            None,
            Some(
                AgentDDSketch::transform_to_sketch(get_aggregated_histogram(
                    sketch3_samples,
                    MetricKind::Incremental,
                ))
                .unwrap(),
            ),
            Some(
                AgentDDSketch::transform_to_sketch(get_aggregated_histogram(
                    sketch4_samples,
                    MetricKind::Incremental,
                ))
                .unwrap(),
            ),
            Some(
                AgentDDSketch::transform_to_sketch(get_aggregated_histogram(
                    sketch5_samples,
                    MetricKind::Incremental,
                ))
                .unwrap(),
            ),
        ];

        assert_normalize(DatadogMetricsNormalizer, agg_histograms, expected_sketches);
    }
}
