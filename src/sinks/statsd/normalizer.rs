use vector_core::event::{Metric, MetricValue};

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
    use std::fmt;

    use vector_core::event::{
        metric::{Bucket, Sample},
        Metric, MetricKind, MetricValue, StatisticKind,
    };

    use super::StatsdNormalizer;
    use crate::sinks::util::buffer::metrics::{MetricNormalize, MetricSet};

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

    fn generate_f64s(start: u16, end: u16) -> Vec<f64> {
        assert!(start <= end);
        let mut samples = Vec::new();
        for n in start..=end {
            samples.push(f64::from(n));
        }
        samples
    }

    fn get_counter(value: f64, kind: MetricKind) -> Metric {
        Metric::new("counter", kind, MetricValue::Counter { value })
    }

    fn get_gauge(value: f64, kind: MetricKind) -> Metric {
        Metric::new("gauge", kind, MetricValue::Gauge { value })
    }

    fn get_set<S, V>(values: S, kind: MetricKind) -> Metric
    where
        S: IntoIterator<Item = V>,
        V: fmt::Display,
    {
        Metric::new(
            "set",
            kind,
            MetricValue::Set {
                values: values.into_iter().map(|i| i.to_string()).collect(),
            },
        )
    }

    fn get_distribution<S, V>(samples: S, kind: MetricKind) -> Metric
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

    fn get_aggregated_histogram<S, V>(samples: S, kind: MetricKind) -> Metric
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

    fn run_comparisons(inputs: Vec<Metric>, expected_outputs: Vec<Option<Metric>>) {
        let mut metric_set = MetricSet::default();
        let mut normalizer = StatsdNormalizer::default();

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

        let expected_gauges = gauges
            .clone()
            .into_iter()
            .map(Option::Some)
            .collect::<Vec<_>>();

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

        let expected_gauges = gauges
            .clone()
            .into_iter()
            .map(Option::Some)
            .collect::<Vec<_>>();

        run_comparisons(gauges, expected_gauges);
    }

    #[test]
    fn absolute_set() {
        let sets = vec![
            get_set(1..=20, MetricKind::Absolute),
            get_set(15..=25, MetricKind::Absolute),
        ];

        let expected_sets = vec![None, Some(get_set(21..=25, MetricKind::Incremental))];

        run_comparisons(sets, expected_sets);
    }

    #[test]
    fn incremental_set() {
        let sets = vec![
            get_set(1..=20, MetricKind::Incremental),
            get_set(15..=25, MetricKind::Incremental),
        ];

        let expected_sets = vec![
            Some(get_set(1..=20, MetricKind::Incremental)),
            Some(get_set(15..=25, MetricKind::Incremental)),
        ];

        run_comparisons(sets, expected_sets);
    }

    #[test]
    fn mixed_set() {
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

        run_comparisons(sets, expected_sets);
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

        run_comparisons(distributions, expected_distributions);
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

        run_comparisons(distributions, expected_distributions);
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

        run_comparisons(distributions, expected_distributions);
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

        run_comparisons(agg_histograms, expected_agg_histograms);
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

        run_comparisons(agg_histograms, expected_agg_histograms);
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

        run_comparisons(agg_histograms, expected_agg_histograms);
    }
}
