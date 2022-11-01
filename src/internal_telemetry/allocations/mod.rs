//! Allocation tracking exposed via internal telemetry.

mod allocator;
use std::{
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::Duration,
};

use arr_macro::arr;

use self::allocator::Tracer;

pub(crate) use self::allocator::{
    without_allocation_tracing, AllocationGroupId, AllocationLayer, GroupedTraceableAllocator,
};

use crossbeam_utils::CachePadded;
/// These arrays represent the allocations and deallocations for each group.
/// We pad each Atomic to reduce false sharing effects.
static GROUP_MEM_ALLOCS: [CachePadded<AtomicU64>; 256] =
    arr![CachePadded::new(AtomicU64::new(0)); 256];
static GROUP_MEM_DEALLOCS: [CachePadded<AtomicU64>; 256] =
    arr![CachePadded::new(AtomicU64::new(0)); 256];

pub type Allocator<A> = GroupedTraceableAllocator<A, MainTracer>;

pub const fn get_grouped_tracing_allocator<A>(allocator: A) -> Allocator<A> {
    GroupedTraceableAllocator::new(allocator, MainTracer)
}

pub struct MainTracer;

impl Tracer for MainTracer {
    #[inline(always)]
    fn trace_allocation(&self, object_size: usize, group_id: AllocationGroupId) {
        GROUP_MEM_ALLOCS[group_id.as_raw()].fetch_add(object_size as u64, Ordering::Relaxed);
    }

    #[inline(always)]
    fn trace_deallocation(&self, object_size: usize, source_group_id: AllocationGroupId) {
        GROUP_MEM_DEALLOCS[source_group_id.as_raw()]
            .fetch_add(object_size as u64, Ordering::Relaxed);
    }
}

/// Initializes allocation tracing.
pub fn init_allocation_tracing() {
    let alloc_processor = thread::Builder::new().name("vector-alloc-processor".to_string());
    alloc_processor
        .spawn(|| {
            without_allocation_tracing(|| loop {
                for idx in 0..GROUP_MEM_ALLOCS.len() {
                    let allocs = GROUP_MEM_ALLOCS[idx].load(Ordering::Relaxed);
                    let deallocs = GROUP_MEM_DEALLOCS[idx].load(Ordering::Relaxed);
                    let mem_used = allocs - deallocs;

                    if mem_used == 0 {
                        continue;
                    }

                    info!(
                        message = "Allocation group memory usage.",
                        group_id = idx,
                        current_memory_allocated_in_bytes = mem_used
                    );
                }
                thread::sleep(Duration::from_millis(10000));
            })
        })
        .unwrap();
}

/// Acquires an allocation group ID.
///
/// This creates an allocation group which allows callers to enter/exit the allocation group context, associating all
/// (de)allocations within the context with that group. An allocation group ID must be "attached" to
/// a [`tracing::Span`] to achieve this" we utilize the logical invariants provided by spans --
/// entering, exiting, and how spans exist as a stack -- in order to handle keeping the "current
/// allocation group" accurate across all threads.
pub fn acquire_allocation_group_id() -> AllocationGroupId {
    AllocationGroupId::register().expect("failed to register allocation group token")
}
