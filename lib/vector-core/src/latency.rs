use std::time::Instant;

use metrics::Histogram;
use vector_common::stats::EwmaGauge;
use vector_common::{
    gauge, histogram,
    internal_event::{GaugeName, HistogramName},
};

use crate::event::EventArray;

const DEFAULT_LATENCY_EWMA_ALPHA: f64 = 0.9;

#[derive(Debug)]
pub struct LatencyRecorder {
    metrics: Option<LatencyMetrics>,
}

#[derive(Debug)]
struct LatencyMetrics {
    histogram: Histogram,
    gauge: EwmaGauge,
}

impl LatencyRecorder {
    pub fn new(ewma_alpha: Option<f64>) -> Self {
        let metrics = crate::config::telemetry()
            .emit_detailed_metrics
            .then(|| LatencyMetrics {
                histogram: histogram!(HistogramName::ComponentLatencySeconds),
                gauge: EwmaGauge::new(
                    gauge!(GaugeName::ComponentLatencyMeanSeconds),
                    ewma_alpha.or(Some(DEFAULT_LATENCY_EWMA_ALPHA)),
                ),
            });
        Self { metrics }
    }

    pub fn on_send(&self, events: &mut EventArray, now: Instant) {
        let Some(metrics) = &self.metrics else {
            return;
        };
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
                metrics.histogram.record(latency);
            }

            metadata.set_last_transform_timestamp(now);
        }
        if count > 0 {
            #[expect(
                clippy::cast_precision_loss,
                reason = "losing precision is acceptable here"
            )]
            let mean = sum / count as f64;
            metrics.gauge.record(mean);
        }
    }
}
