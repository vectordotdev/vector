use vector_core::event::{Metric, MetricValue};
use crate::sinks::util::buffer::metrics::{MetricNormalize, MetricSet};

#[derive(Default)]
pub(crate) struct ClickHouseMetricsNormalizer;

impl MetricNormalize for ClickHouseMetricsNormalizer {
    fn normalize(&mut self, state: &mut MetricSet, metric: Metric) -> Option<Metric> {
        // We primarily care about making sure that counters are incremental, and that gauges are
        // always absolute.  For other metric kinds, we want them to be incremental.
        match &metric.value() {
            MetricValue::Counter { .. } => state.make_incremental(metric),
            MetricValue::Gauge { .. } => state.make_absolute(metric),
            // We convert distributions and aggregated histograms to sketches internally. We can't
            // send absolute sketches to Datadog, though, so we incrementalize them first.
            // MetricValue::Distribution { .. } => state
            //     .make_incremental(metric)
            //     .filter(|metric| !metric.value().is_empty())
            //     .and_then(|metric| AgentDDSketch::transform_to_sketch(metric).ok()),
            // MetricValue::AggregatedHistogram { .. } => state
            //     .make_incremental(metric)
            //     .filter(|metric| !metric.value().is_empty())
            //     .and_then(|metric| AgentDDSketch::transform_to_sketch(metric).ok()),
            // Sketches cannot be subtracted from one another, so we treat them as implicitly
            // incremental, and just update the metric type.
            // MetricValue::Sketch { .. } => Some(metric.into_incremental()),
            // Otherwise, ensure that it's incremental.
            _ => None,
        }
    }
}
