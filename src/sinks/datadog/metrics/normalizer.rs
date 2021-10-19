use vector_core::{
    event::{Metric, MetricKind, MetricValue},
    metrics::AgentDDSketch,
};

use crate::sinks::util::buffer::metrics::{MetricNormalize, MetricSet};

pub struct DatadogMetricsNormalizer;

impl MetricNormalize for DatadogMetricsNormalizer {
    fn apply_state(state: &mut MetricSet, metric: Metric) -> Option<Metric> {
        // We primarily care about making sure that counters are incremental, and that gauges are
        // always absolute.  For other metric kinds, we want them to be incremental.
        match &metric.value() {
            MetricValue::Counter { .. } => state.make_incremental(metric),
            MetricValue::Gauge { .. } => state.make_absolute(metric),
            // We convert distributions and aggregated histograms to sketches internally. We can't
            // send absolute sketches to Datadog, though, so we incrementalize them first.
            MetricValue::Distribution { .. } | MetricValue::AggregatedHistogram { .. } => state
                .make_incremental(metric)
                .map(AgentDDSketch::transform_to_sketch),
            _ => match metric.kind() {
                MetricKind::Absolute => state.make_incremental(metric),
                MetricKind::Incremental => Some(metric),
            },
        }
    }
}
