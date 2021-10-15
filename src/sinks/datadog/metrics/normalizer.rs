use std::mem;

use vector_core::{event::{Metric, MetricKind, MetricValue, metric::MetricSketch}, metrics::AgentDDSketch};

use crate::sinks::util::buffer::metrics::{MetricNormalize, MetricSet};

pub struct DatadogMetricsNormalizer;

impl MetricNormalize for DatadogMetricsNormalizer {
    fn apply_state(state: &mut MetricSet, metric: Metric) -> Option<Metric> {
        // We primarily care about making sure that counters are incremental, and that gauges are
        // always absolute.  For other metric kinds, we want them to be incremental.
        match &metric.value() {
            MetricValue::Counter { .. } => state.make_incremental(metric),
            MetricValue::Gauge { .. } => state.make_absolute(metric),
            MetricValue::AggregatedHistogram { .. } => {
                // Sketches should be sent to Datadog in an incremental fashion, so we need to
                // incrementalize the aggregated histogram first and then generate a sketch from it.
                state.make_incremental(metric)
                    .map(|metric| {
                        let (series, data, metadata) = metric.into_parts();

                        let sketch = match data.value_mut() {
                            MetricValue::AggregatedHistogram { buckets, .. } => {
                                let delta_buckets = mem::replace(buckets, Vec::new());
                                let sketch = AgentDDSketch::with_agent_defaults();
                                sketch.insert_interpolate_buckets(delta_buckets);
                                sketch
                            },
                            // We should never get back a different metric value simply from converting
                            // between absolute and incremental.
                            _ => unreachable!(),
                        };

                        let _ = mem::replace(data.value_mut(), MetricValue::Sketch {
                            sketch: MetricSketch::AgentDDSketch(sketch),
                        });

                        Metric::from_parts(series, data, metadata)
                    })
            },
            _ => match metric.kind() {
                MetricKind::Absolute => state.make_incremental(metric),
                MetricKind::Incremental => Some(metric),
            }
        }
    }
}
