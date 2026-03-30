use std::time::Instant;

use metrics::{Histogram, gauge, histogram};
use vector_common::stats::EwmaGauge;

use crate::event::EventArray;

const COMPONENT_LATENCY: &str = "component_latency_seconds";
const COMPONENT_LATENCY_MEAN: &str = "component_latency_mean_seconds";
const DEFAULT_LATENCY_EWMA_ALPHA: f64 = 0.9;

#[derive(Debug)]
pub struct LatencyRecorder {
    histogram: Histogram,
    gauge: EwmaGauge,
}

impl LatencyRecorder {
    pub fn new(ewma_alpha: Option<f64>) -> Self {
        Self {
            histogram: histogram!(COMPONENT_LATENCY),
            gauge: EwmaGauge::new(
                gauge!(COMPONENT_LATENCY_MEAN),
                ewma_alpha.or(Some(DEFAULT_LATENCY_EWMA_ALPHA)),
            ),
        }
    }

    pub fn on_send(&self, events: &mut EventArray, now: Instant) {
        let mut sum = 0.0;
        let mut count = 0usize;

        // Since all of the events in the array will most likely have entered and exited the
        // component at close to the same time, we average all the latencies over the entire array
        // and record it just once in the EWMA-backed gauge. If we were to record each latency
        // individually, the gauge would effectively just reflect the latest array's latency,
        // eliminating the utility of the EWMA averaging. However, we record the individual
        // latencies in the histogram to get a more granular view of the latency distribution.
        for mut event in events.iter_events_mut() {
            let metadata = event.metadata_mut();
            if let Some(previous) = metadata.last_transform_timestamp() {
                let latency = now.saturating_duration_since(previous).as_secs_f64();
                sum += latency;
                count += 1;
                self.histogram.record(latency);
            }

            metadata.set_last_transform_timestamp(now);
        }
        if count > 0 {
            #[expect(
                clippy::cast_precision_loss,
                reason = "losing precision is acceptable here"
            )]
            let mean = sum / count as f64;
            self.gauge.record(mean);
        }
    }
}
