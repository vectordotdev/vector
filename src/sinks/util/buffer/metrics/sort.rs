use crate::event::Metric;

/// Sorts metrics in an order that is likely to achieve good compression.
pub fn sort_for_compression(metrics: &mut [Metric]) {
    // This just sorts by series today. This tends to compress better than a random ordering by
    // 2-3x (JSON encoded, deflate algorithm)
    metrics.sort_unstable_by(|a, b| a.series().cmp(b.series()))
}

#[cfg(test)]
mod test {
    use rand::{prelude::SliceRandom, thread_rng};
    use vector_core::{
        event::{Metric, MetricKind},
        metric_tags,
    };

    use crate::event::MetricValue;

    // This just ensures the sorting does not change. `sort_for_compression` relies on
    // the default `PartialOrd` on `MetricSeries`.
    #[test]
    fn test_compression_order() {
        let sorted_metrics = vec![
            Metric::new(
                "metric_1",
                MetricKind::Absolute,
                MetricValue::Gauge { value: 0.0 },
            ),
            Metric::new(
                "metric_2",
                MetricKind::Incremental,
                MetricValue::Gauge { value: 0.0 },
            ),
            Metric::new(
                "metric_3",
                MetricKind::Absolute,
                MetricValue::Gauge { value: 0.0 },
            )
            .with_tags(Some(metric_tags!("z" => "z"))),
            Metric::new(
                "metric_4",
                MetricKind::Absolute,
                MetricValue::Gauge { value: 0.0 },
            )
            .with_tags(Some(metric_tags!("a" => "a"))),
            Metric::new(
                "metric_4",
                MetricKind::Absolute,
                MetricValue::Gauge { value: 0.0 },
            )
            .with_tags(Some(metric_tags!(
                "a" => "a",
                "b" => "b",
            ))),
            Metric::new(
                "metric_4",
                MetricKind::Absolute,
                MetricValue::Gauge { value: 0.0 },
            )
            .with_tags(Some(metric_tags!("b" => "b"))),
        ];

        let mut rand_metrics = sorted_metrics.clone();
        rand_metrics.shuffle(&mut thread_rng());
        super::sort_for_compression(&mut rand_metrics);
        assert_eq!(sorted_metrics, rand_metrics);
    }
}
