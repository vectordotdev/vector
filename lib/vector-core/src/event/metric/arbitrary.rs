use proptest::{collection::btree_set, prelude::*};

use crate::metrics::AgentDDSketch;

use super::{Bucket, MetricSketch, MetricValue, Quantile, Sample, StatisticKind};

fn arb_metric_value() -> BoxedStrategy<MetricValue> {
    let strategy = prop_oneof![
        any::<f64>().prop_map(|value| MetricValue::Counter { value }),
        any::<f64>().prop_map(|value| MetricValue::Gauge { value }),
        btree_set("[a-z0-9]{8,16}", 2..16).prop_map(|values| MetricValue::Set { values }),
        any::<(Vec<Sample>, StatisticKind)>()
            .prop_map(|(samples, statistic)| MetricValue::Distribution { samples, statistic }),
        any::<(Vec<Bucket>, u64, f64)>().prop_map(|(buckets, count, sum)| {
            MetricValue::AggregatedHistogram {
                buckets,
                count,
                sum,
            }
        }),
        any::<(Vec<Quantile>, u64, f64)>().prop_map(|(quantiles, count, sum)| {
            MetricValue::AggregatedSummary {
                quantiles,
                count,
                sum,
            }
        }),
        any::<MetricSketch>().prop_map(|sketch| MetricValue::Sketch { sketch }),
    ];
    strategy.boxed()
}

impl Arbitrary for MetricValue {
    type Parameters = ();
    type Strategy = BoxedStrategy<MetricValue>;

    fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
        arb_metric_value()
    }
}

fn arb_metric_sketch() -> BoxedStrategy<MetricSketch> {
    let strategy = prop_oneof![any::<AgentDDSketch>().prop_map(MetricSketch::AgentDDSketch),];
    strategy.boxed()
}

impl Arbitrary for MetricSketch {
    type Parameters = ();
    type Strategy = BoxedStrategy<MetricSketch>;

    fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
        arb_metric_sketch()
    }
}

fn arb_statistic_kind() -> BoxedStrategy<StatisticKind> {
    let strategy = prop_oneof![Just(StatisticKind::Histogram), Just(StatisticKind::Summary),];
    strategy.boxed()
}

impl Arbitrary for StatisticKind {
    type Parameters = ();
    type Strategy = BoxedStrategy<StatisticKind>;

    fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
        arb_statistic_kind()
    }
}

fn arb_sample() -> BoxedStrategy<Sample> {
    any::<(f64, u32)>()
        .prop_map(|(value, rate)| Sample { value, rate })
        .boxed()
}

impl Arbitrary for Sample {
    type Parameters = ();
    type Strategy = BoxedStrategy<Sample>;

    fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
        arb_sample()
    }
}

fn arb_bucket() -> BoxedStrategy<Bucket> {
    any::<(f64, u64)>()
        .prop_map(|(upper_limit, count)| Bucket { upper_limit, count })
        .boxed()
}

impl Arbitrary for Bucket {
    type Parameters = ();
    type Strategy = BoxedStrategy<Bucket>;

    fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
        arb_bucket()
    }
}

fn arb_quantile() -> BoxedStrategy<Quantile> {
    any::<(f64, f64)>()
        .prop_map(|(quantile, value)| Quantile { quantile, value })
        .boxed()
}

impl Arbitrary for Quantile {
    type Parameters = ();
    type Strategy = BoxedStrategy<Quantile>;

    fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
        arb_quantile()
    }
}
