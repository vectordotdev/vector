//! Allocation tracking exposed via internal telemetry.

mod allocator;
use std::{
    ops::Index,
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::Duration,
};

use arr_macro::arr;

use self::allocator::{without_allocation_tracing, Tracer};

pub(crate) use self::allocator::{
    AllocationGroupId, AllocationGroupToken, AllocationLayer, GroupedTraceableAllocator,
};

static GROUP_MEM_METRICS: [AtomicU64; 256] = arr![AtomicU64::new(0); 256];

pub type Allocator<A> = GroupedTraceableAllocator<A, MainTracer>;

pub const fn get_grouped_tracing_allocator<A>(allocator: A) -> Allocator<A> {
    GroupedTraceableAllocator::new(allocator, MainTracer)
}

pub struct MainTracer;

impl Tracer for MainTracer {
    #[inline(always)]
    fn trace_allocation(&self, _wrapped_size: usize, _group_id: AllocationGroupId) {}

    #[inline(always)]
    fn trace_deallocation(&self, _wrapped_size: usize, _source_group_id: AllocationGroupId) {}
}

/// Initializes allocation tracing.
pub fn init_allocation_tracing() {
    let alloc_processor = thread::Builder::new().name("vector-alloc-processor".to_string());
    alloc_processor
        .spawn(move || {
            without_allocation_tracing(move || loop {
                for idx in 0..GROUP_MEM_METRICS.len() {
                    let atomic_ref = GROUP_MEM_METRICS.index(idx);
                    let mem_used = atomic_ref.load(Ordering::Relaxed);
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
pub fn acquire_allocation_group_id() -> AllocationGroupToken {
    let group_id =
        AllocationGroupToken::register().expect("failed to register allocation group token");
    // We default to the root group in case of overflow
    if group_id.id().as_usize().get() >= GROUP_MEM_METRICS.len() {
        AllocationGroupToken(AllocationGroupId::ROOT)
    } else {
        group_id
    }
}
