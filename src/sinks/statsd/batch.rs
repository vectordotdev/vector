use vector_lib::event::Metric;
use vector_lib::stream::batcher::limiter::ItemBatchSize;

// This accounts for the separators, the metric type string, the length of the value itself. It can
// never be too small, as the above values will always take at least 4 bytes.
const EST_OVERHEAD_LEN: usize = 4;

#[derive(Default)]
pub(super) struct StatsdBatchSizer;

impl ItemBatchSize<Metric> for StatsdBatchSizer {
    fn size(&self, item: &Metric) -> usize {
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
        // Estimated overhead (separators, metric value, etc)
        + EST_OVERHEAD_LEN
    }
}
