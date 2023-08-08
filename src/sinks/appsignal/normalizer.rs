use vector_core::event::{Metric, MetricValue};

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

    use vector_core::event::{Metric, MetricKind, MetricValue};

    use super::AppsignalMetricsNormalizer;
    use crate::sinks::util::buffer::metrics::{MetricNormalize, MetricSet};

    fn get_counter(value: f64, kind: MetricKind) -> Metric {
        Metric::new("counter", kind, MetricValue::Counter { value })
    }

    fn get_gauge(value: f64, kind: MetricKind) -> Metric {
        Metric::new("gauge", kind, MetricValue::Gauge { value })
    }

    fn run_comparisons(inputs: Vec<Metric>, expected_outputs: Vec<Option<Metric>>) {
        let mut metric_set = MetricSet::default();
        let mut normalizer = AppsignalMetricsNormalizer;

        for (input, expected) in inputs.into_iter().zip(expected_outputs) {
            let result = normalizer.normalize(&mut metric_set, input);
            assert_eq!(result, expected);
        }
    }

    #[test]
    fn absolute_counter() {
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

        run_comparisons(counters, expected_counters);
    }

    #[test]
    fn incremental_counter() {
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

        run_comparisons(counters, expected_counters);
    }

    #[test]
    fn mixed_counter() {
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

        run_comparisons(counters, expected_counters);
    }

    #[test]
    fn absolute_gauge() {
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

        run_comparisons(gauges, expected_gauges);
    }

    #[test]
    fn incremental_gauge() {
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

        run_comparisons(gauges, expected_gauges);
    }

    #[test]
    fn mixed_gauge() {
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

        run_comparisons(gauges, expected_gauges);
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

        run_comparisons(vec![metric], vec![None]);
    }
}
