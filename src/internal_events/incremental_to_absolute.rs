use metrics::{counter, gauge};
use vector_lib::internal_event::InternalEvent;

/// Emitted to track the current size of the metrics cache in bytes
#[derive(Debug)]
pub struct IncrementalToAbsoluteMetricsCache {
    pub size: usize,
    pub count: usize,
    pub evictions: usize,
    pub has_capacity_policy: bool,
}

impl InternalEvent for IncrementalToAbsoluteMetricsCache {
    fn emit(self) {
        trace!(
            message = "Metrics cache stats.",
            size = %self.size,
            count = %self.count,
            evictions = %self.evictions,
            has_capacity_policy = %self.has_capacity_policy,
        );
        
        // Only emit component_cache_bytes if capacity policy is defined
        if self.has_capacity_policy {
            gauge!("component_cache_bytes")
                .set(self.size as f64);
        }
        
        gauge!("component_cache_events")
            .set(self.count as f64);
        
        counter!("component_cache_evictions_total")
            .increment(self.evictions as u64);
    }
}
