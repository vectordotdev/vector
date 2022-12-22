use std::{
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    thread,
    time::Duration,
};

mod accumulator;
mod allocator;
mod group;
mod sharded;
pub(self) mod stack;
mod tracing;

use metrics::{counter, decrement_gauge, increment_gauge};

pub use self::allocator::GroupedTracingAllocator;
pub use self::group::*;
pub use self::tracing::AllocationLayer;

// Whether or not to trace (de)allocations.
pub static TRACE_ALLOCATIONS: AtomicBool = AtomicBool::new(false);

// Reporting interval of (de)allocation statistics, in milliseconds.
pub static REPORTING_INTERVAL_MS: AtomicU64 = AtomicU64::new(500);

/// Initializes allocation tracing.
pub fn init_allocation_tracing_reporter() {
    let alloc_processor = thread::Builder::new().name("alloc-reporter".to_string());
    alloc_processor
        .spawn(|| loop {
            let groups = get_registered_allocation_groups();
            for group in groups {
                let (allocations_diff, deallocations_diff) = group.consume_and_reset_statistics();
                if allocations_diff == 0 && deallocations_diff == 0 {
                    continue;
                }

                let mem_used_diff = allocations_diff as i64 - deallocations_diff as i64;
                if allocations_diff > 0 {
                    counter!(
                        "component_allocated_bytes_total",
                        allocations_diff,
                        "component_kind" => group.component_kind.clone(),
                        "component_type" => group.component_type.clone(),
                        "component_id" => group.component_id.clone());
                }
                if deallocations_diff > 0 {
                    counter!(
                        "component_deallocated_bytes_total",
                        deallocations_diff,
                        "component_kind" => group.component_kind.clone(),
                        "component_type" => group.component_type.clone(),
                        "component_id" => group.component_id.clone());
                }
                if mem_used_diff > 0 {
                    increment_gauge!(
                        "component_allocated_bytes",
                        mem_used_diff as f64,
                        "component_kind" => group.component_kind.clone(),
                        "component_type" => group.component_type.clone(),
                        "component_id" => group.component_id.clone());
                }
                if mem_used_diff < 0 {
                    decrement_gauge!(
                        "component_allocated_bytes",
                        -mem_used_diff as f64,
                        "component_kind" => group.component_kind.clone(),
                        "component_type" => group.component_type.clone(),
                        "component_id" => group.component_id.clone());
                }
            }

            thread::sleep(Duration::from_millis(
                REPORTING_INTERVAL_MS.load(Ordering::Relaxed),
            ));
        })
        .unwrap();
}
