use super::request_builder::{
    DISTRIBUTION_QUANTILES, DISTRIBUTION_STAT_FIELD_COUNT, SUMMARY_STAT_FIELD_COUNT,
};
use vector_lib::{
    event::{Metric, MetricValue},
    stream::batcher::limiter::ItemBatchSize,
};

const F64_BYTE_SIZE: usize = 8;
const I64_BYTE_SIZE: usize = 8;

/// GreptimeDBBatchSizer is a batch sizer for metrics.
#[derive(Default)]
pub struct GreptimeDBBatchSizer;

impl GreptimeDBBatchSizer {
    pub fn estimated_size_of(&self, item: &Metric) -> usize {
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
            // timestamp
            + I64_BYTE_SIZE
            +
        // value size
            match item.value() {
                MetricValue::Counter { .. } | MetricValue::Gauge { .. } | MetricValue::Set { ..} => F64_BYTE_SIZE,
                MetricValue::Distribution { .. } => F64_BYTE_SIZE * (DISTRIBUTION_QUANTILES.len() + DISTRIBUTION_STAT_FIELD_COUNT),
                MetricValue::AggregatedHistogram { buckets, .. }  => F64_BYTE_SIZE * (buckets.len() + SUMMARY_STAT_FIELD_COUNT),
                MetricValue::AggregatedSummary { quantiles, .. } => F64_BYTE_SIZE * (quantiles.len() + SUMMARY_STAT_FIELD_COUNT),
                MetricValue::Sketch { .. } => F64_BYTE_SIZE * (DISTRIBUTION_QUANTILES.len() + DISTRIBUTION_STAT_FIELD_COUNT),
            }
    }
}

impl ItemBatchSize<Metric> for GreptimeDBBatchSizer {
    fn size(&self, item: &Metric) -> usize {
        self.estimated_size_of(item)
    }
}
