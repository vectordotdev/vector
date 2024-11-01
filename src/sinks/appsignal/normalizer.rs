use vector_lib::event::{Metric, MetricValue};

use crate::sinks::util::buffer::metrics::{MetricNormalize, MetricSet};

#[derive(Default)]
pub(crate) struct AppsignalMetricsNormalizer;

impl MetricNormalize for AppsignalMetricsNormalizer {
    fn normalize(&mut self, state: &mut MetricSet, metric: Metric) -> Option<Metric> {
        // We only care about making sure that counters are incremental, and that gauges are
        // always absolute. Other metric types are currently unsupported.
        match &metric.value() {
            // We always send counters as incremental and gauges as absolute. Realistically, any
            // system sending an incremental gauge update is kind of doing it wrong, but alas.
            MetricValue::Counter { .. } => state.make_incremental(metric),
            MetricValue::Gauge { .. } => state.make_absolute(metric),
            // Otherwise, send it through as-is.
            _ => Some(metric),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use crate::event::{Metric, MetricKind, MetricValue};

    use super::AppsignalMetricsNormalizer;
    use crate::test_util::metrics::{assert_normalize, tests};

    #[test]
    fn absolute_counter() {
        tests::absolute_counter_normalize_to_incremental(AppsignalMetricsNormalizer);
    }

    #[test]
    fn incremental_counter() {
        tests::incremental_counter_normalize_to_incremental(AppsignalMetricsNormalizer);
    }

    #[test]
    fn mixed_counter() {
        tests::mixed_counter_normalize_to_incremental(AppsignalMetricsNormalizer);
    }

    #[test]
    fn absolute_gauge() {
        tests::absolute_gauge_normalize_to_absolute(AppsignalMetricsNormalizer);
    }

    #[test]
    fn incremental_gauge() {
        tests::incremental_gauge_normalize_to_absolute(AppsignalMetricsNormalizer);
    }

    #[test]
    fn mixed_gauge() {
        tests::mixed_gauge_normalize_to_absolute(AppsignalMetricsNormalizer);
    }

    #[test]
    fn other_metrics() {
        let metric = Metric::new(
            "set",
            MetricKind::Incremental,
            MetricValue::Set {
                values: BTreeSet::new(),
            },
        );

        assert_normalize(
            AppsignalMetricsNormalizer,
            vec![metric.clone()],
            vec![Some(metric)],
        );
    }
}
