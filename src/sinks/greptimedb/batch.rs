use vector_core::{
    event::{Metric, MetricValue},
    stream::batcher::limiter::ItemBatchSize,
};

#[derive(Default)]
pub(super) struct GreptimeDBBatchSizer;

impl GreptimeDBBatchSizer {
    pub(super) fn estimated_size_of(&self, item: &Metric) -> usize {
        // Metric name.
        item.series().name().name().len()
        // Metric namespace, with an additional 1 to account for the namespace separator.
        + item.series().name().namespace().map(|s| s.len() + 1).unwrap_or(0)
        // Metric tags, with an additional 1 per tag to account for the tag key/value separator.
        + item.series().tags().map(|t| {
            t.iter_all().map(|(k, v)| {
                k.len() + 1 + v.map(|v| v.len()).unwrap_or(0)
            })
            .sum()
        })
            .unwrap_or(0)
            +
        // value size
            match item.value() {
                MetricValue::Counter { .. } | MetricValue::Gauge { .. } | MetricValue::Set { ..} => 8,
                MetricValue::Distribution { .. } => 8 * 10,
                MetricValue::AggregatedHistogram { buckets, .. }  => 8 * (buckets.len() + 2),
                MetricValue::AggregatedSummary { quantiles, .. } => 8 * (quantiles.len() + 2),
                MetricValue::Sketch { .. } => 8 * 10,
            }
    }
}

impl ItemBatchSize<Metric> for GreptimeDBBatchSizer {
    fn size(&self, item: &Metric) -> usize {
        self.estimated_size_of(item)
    }
}
