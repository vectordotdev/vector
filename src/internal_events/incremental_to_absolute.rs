use metrics::{counter, gauge};
use vector_lib::internal_event::InternalEvent;

/// Emitted to track the current size of the metrics cache in bytes
#[derive(Debug)]
pub struct IncrementalToAbsoluteMetricsCache {
    pub size: usize,
    pub count: usize,
    pub evictions: usize,
}

impl InternalEvent for IncrementalToAbsoluteMetricsCache {
    fn emit(self) {
        trace!(
            message = "Metrics cache stats.",
            size = %self.size,
            count = %self.count,
            evictions = %self.evictions,
        );
        gauge!("component_cache_bytes", "component_type" => "transform", "transform_type" => "incremental_to_absolute")
            .set(self.size as f64);
        gauge!("component_cache_events", "component_type" => "transform", "transform_type" => "incremental_to_absolute")
            .set(self.count as f64);
        counter!("component_cache_evictions_total", "component_type" => "transform", "transform_type" => "incremental_to_absolute")
            .increment(self.evictions as u64);
    }
}
