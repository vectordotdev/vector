use vector_lib::event::{Metric, MetricValue};

use crate::sinks::util::buffer::metrics::{MetricNormalize, MetricSet};

#[derive(Default)]
pub(crate) struct StatsdNormalizer;

impl MetricNormalize for StatsdNormalizer {
    fn normalize(&mut self, state: &mut MetricSet, metric: Metric) -> Option<Metric> {
        // We primarily care about making sure that metrics are incremental, but for gauges, we can
        // handle both incremental and absolute versions during encoding.
        match metric.value() {
            // Pass through gauges as-is.
            MetricValue::Gauge { .. } => Some(metric),
            // Otherwise, ensure that it's incremental.
            _ => state.make_incremental(metric),
        }
    }
}

#[cfg(test)]
mod tests {
    use vector_lib::event::MetricKind;

    use super::StatsdNormalizer;

    use crate::test_util::metrics::{
        assert_normalize, generate_f64s, get_aggregated_histogram, get_distribution, get_gauge,
        tests,
    };

    #[test]
    fn absolute_counter() {
        tests::absolute_counter_normalize_to_incremental(StatsdNormalizer);
    }

    #[test]
    fn incremental_counter() {
        tests::incremental_counter_normalize_to_incremental(StatsdNormalizer);
    }

    #[test]
    fn mixed_counter() {
        tests::mixed_counter_normalize_to_incremental(StatsdNormalizer);
    }

    #[test]
    fn absolute_gauge() {
        tests::absolute_gauge_normalize_to_absolute(StatsdNormalizer);
    }

    #[test]
    fn incremental_gauge() {
        let first_value = 3.14;
        let second_value = 8.675309;

        let gauges = vec![
            get_gauge(first_value, MetricKind::Incremental),
            get_gauge(second_value, MetricKind::Incremental),
        ];

        let expected_gauges = gauges
            .clone()
            .into_iter()
            .map(Option::Some)
            .collect::<Vec<_>>();

        assert_normalize(StatsdNormalizer, gauges, expected_gauges);
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

        let expected_gauges = gauges
            .clone()
            .into_iter()
            .map(Option::Some)
            .collect::<Vec<_>>();

        assert_normalize(StatsdNormalizer, gauges, expected_gauges);
    }

    #[test]
    fn absolute_set() {
        tests::absolute_set_normalize_to_incremental(StatsdNormalizer);
    }

    #[test]
    fn incremental_set() {
        tests::incremental_set_normalize_to_incremental(StatsdNormalizer);
    }

    #[test]
    fn mixed_set() {
        tests::mixed_set_normalize_to_incremental(StatsdNormalizer);
    }

    #[test]
    fn absolute_distribution() {
        let samples1 = generate_f64s(1, 100);
        let samples2 = generate_f64s(1, 125);
        let expected_samples = generate_f64s(101, 125);

        let distributions = vec![
            get_distribution(samples1, MetricKind::Absolute),
            get_distribution(samples2, MetricKind::Absolute),
        ];

        let expected_distributions = vec![
            None,
            Some(get_distribution(expected_samples, MetricKind::Incremental)),
        ];

        assert_normalize(StatsdNormalizer, distributions, expected_distributions);
    }

    #[test]
    fn incremental_distribution() {
        let samples1 = generate_f64s(1, 100);
        let samples2 = generate_f64s(75, 125);

        let distributions = vec![
            get_distribution(samples1, MetricKind::Incremental),
            get_distribution(samples2, MetricKind::Incremental),
        ];

        let expected_distributions = distributions.iter().cloned().map(Some).collect();

        assert_normalize(StatsdNormalizer, distributions, expected_distributions);
    }

    #[test]
    fn mixed_distribution() {
        let samples1 = generate_f64s(1, 100);
        let samples2 = generate_f64s(75, 125);
        let samples3 = generate_f64s(75, 187);
        let samples4 = generate_f64s(22, 45);
        let samples5 = generate_f64s(1, 100);

        let distributions = vec![
            get_distribution(samples1, MetricKind::Incremental),
            get_distribution(samples2, MetricKind::Absolute),
            get_distribution(samples3, MetricKind::Absolute),
            get_distribution(samples4, MetricKind::Incremental),
            get_distribution(samples5, MetricKind::Incremental),
        ];

        let expected_distributions = vec![
            Some(distributions[0].clone()),
            None,
            Some(get_distribution(
                generate_f64s(126, 187),
                MetricKind::Incremental,
            )),
            Some(distributions[3].clone()),
            Some(distributions[4].clone()),
        ];

        assert_normalize(StatsdNormalizer, distributions, expected_distributions);
    }

    #[test]
    fn absolute_aggregated_histogram() {
        let samples1 = generate_f64s(1, 100);
        let samples2 = generate_f64s(1, 125);

        let agg_histograms = vec![
            get_aggregated_histogram(samples1, MetricKind::Absolute),
            get_aggregated_histogram(samples2, MetricKind::Absolute),
        ];

        let expected_agg_histograms = vec![];

        assert_normalize(StatsdNormalizer, agg_histograms, expected_agg_histograms);
    }

    #[test]
    fn incremental_aggregated_histogram() {
        let samples1 = generate_f64s(1, 100);
        let samples2 = generate_f64s(1, 125);

        let agg_histograms = vec![
            get_aggregated_histogram(samples1, MetricKind::Incremental),
            get_aggregated_histogram(samples2, MetricKind::Incremental),
        ];

        let expected_agg_histograms = agg_histograms
            .clone()
            .into_iter()
            .map(Option::Some)
            .collect::<Vec<_>>();

        assert_normalize(StatsdNormalizer, agg_histograms, expected_agg_histograms);
    }

    #[test]
    fn mixed_aggregated_histogram() {
        let samples1 = generate_f64s(1, 100);
        let samples2 = generate_f64s(75, 125);
        let samples3 = generate_f64s(75, 187);
        let samples4 = generate_f64s(22, 45);
        let samples5 = generate_f64s(1, 100);

        let agg_histograms = vec![
            get_aggregated_histogram(samples1, MetricKind::Incremental),
            get_aggregated_histogram(samples2, MetricKind::Absolute),
            get_aggregated_histogram(samples3, MetricKind::Absolute),
            get_aggregated_histogram(samples4, MetricKind::Incremental),
            get_aggregated_histogram(samples5, MetricKind::Incremental),
        ];

        let expected_agg_histograms = vec![];

        assert_normalize(StatsdNormalizer, agg_histograms, expected_agg_histograms);
    }
}
