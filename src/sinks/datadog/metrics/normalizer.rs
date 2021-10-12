use vector_core::event::Metric;

use crate::sinks::util::buffer::metrics::{MetricNormalize, MetricSet};

pub struct DatadogMetricsNormalizer;

impl MetricNormalize for DatadogMetricsNormalizer {
    fn apply_state(state: &mut MetricSet, metric: Metric) -> Option<Metric> {
        todo!()
    }
}
