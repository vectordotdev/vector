use proptest::{collection::btree_set, prelude::*};

use crate::metrics::AgentDDSketch;

use super::{
    samples_to_buckets, Bucket, MetricSketch, MetricValue, Quantile, Sample, StatisticKind,
};

fn realistic_float() -> proptest::num::f64::Any {
    proptest::num::f64::POSITIVE | proptest::num::f64::NEGATIVE | proptest::num::f64::ZERO
}

fn arb_metric_value() -> BoxedStrategy<MetricValue> {
    let strategy = prop_oneof![
        realistic_float().prop_map(|value| MetricValue::Counter { value }),
        realistic_float().prop_map(|value| MetricValue::Gauge { value }),
        btree_set("[a-z0-9]{8,16}", 2..16).prop_map(|values| MetricValue::Set { values }),
        any::<(Vec<Sample>, StatisticKind)>()
            .prop_map(|(samples, statistic)| MetricValue::Distribution { samples, statistic }),
        any::<Vec<Sample>>().prop_map(|samples| {
            // Hard-coded log2 buckets for the sake of testing.
            let (buckets, count, sum) =
                samples_to_buckets(&samples, &[0.5, 1.0, 2.0, 4.0, 8.0, 16.0, 32.0]);

            MetricValue::AggregatedHistogram {
                buckets,
                count,
                sum,
            }
        }),
        any::<AgentDDSketch>().prop_map(|sketch| {
            // We lean on `AgentDDSketch` to generate our quantiles and the count/sum.
            let count = u64::from(sketch.count());
            let sum = sketch.sum().unwrap_or(0.0);
            let quantiles = [0.5, 0.95, 0.99, 0.999]
                .iter()
                .copied()
                .map(|quantile| {
                    let value = sketch.quantile(quantile).unwrap_or(0.0);
                    Quantile { quantile, value }
                })
                .collect::<Vec<_>>();

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
    let strategy = prop_oneof![Just(StatisticKind::Histogram), Just(StatisticKind::Summary)];
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
    (realistic_float(), any::<u32>())
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
    (realistic_float(), any::<u64>())
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
    (0.0..=1.0, realistic_float())
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

fn arb_agent_ddsketch() -> BoxedStrategy<AgentDDSketch> {
    use proptest::collection::vec as arb_vec;

    arb_vec(realistic_float(), 16..128)
        .prop_map(|samples| {
            let mut sketch = AgentDDSketch::with_agent_defaults();
            sketch.insert_many(&samples);
            sketch
        })
        .boxed()
}

impl Arbitrary for AgentDDSketch {
    type Parameters = ();
    type Strategy = BoxedStrategy<AgentDDSketch>;

    fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
        arb_agent_ddsketch()
    }
}
