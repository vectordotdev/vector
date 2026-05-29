use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use metrics::Histogram;
use vector_common::{histogram, internal_event::HistogramName};

#[derive(Debug)]
pub struct SchedulingPressureRecorder {
    histogram: Histogram,
    concurrency_limit: usize,
    completed: Arc<AtomicU64>,
    yielded_since_last_sample: u64,
}

impl SchedulingPressureRecorder {
    pub fn new(component_id: &str, concurrency_limit: usize) -> Self {
        Self {
            histogram: histogram!(
                HistogramName::EstimatedConcurrentTransformSchedulingPressure,
                "component_id" => component_id.to_owned(),
            ),
            concurrency_limit,
            completed: Arc::new(AtomicU64::new(0)),
            yielded_since_last_sample: 0,
        }
    }

    pub fn on_task_yielded(&mut self) {
        self.yielded_since_last_sample += 1;
    }

    pub fn task_completion_signal(&self) -> impl FnOnce() + Send + 'static {
        let completed = Arc::clone(&self.completed);
        move || {
            completed.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn record_sample(&mut self) {
        let count = self
            .completed
            .fetch_sub(self.yielded_since_last_sample, Ordering::Relaxed);
        self.yielded_since_last_sample = 0;
        #[expect(
            clippy::cast_precision_loss,
            reason = "metric value, precision loss acceptable"
        )]
        let pressure = (count as f64 / self.concurrency_limit as f64).min(1.0);
        self.histogram.record(pressure);
    }
}
