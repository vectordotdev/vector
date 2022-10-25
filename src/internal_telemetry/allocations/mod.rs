//! Allocation tracking exposed via internal telemetry.

mod allocator;

use self::allocator::Tracer;
pub(crate) use self::allocator::{AllocationGroupId, AllocationLayer, GroupedTraceableAllocator};

pub type Allocator<A> = GroupedTraceableAllocator<A, MainTracer>;

pub const fn get_grouped_tracing_allocator<A>(allocator: A) -> Allocator<A> {
    GroupedTraceableAllocator::new(allocator, MainTracer)
}

pub struct MainTracer;

impl Tracer for MainTracer {
    #[inline(always)]
    fn trace_allocation(&self, _object_size: usize, _group_id: AllocationGroupId) {}

    #[inline(always)]
    fn trace_deallocation(&self, _object_size: usize, _source_group_id: AllocationGroupId) {}
}

/// Initializes allocation tracing.
pub const fn init_allocation_tracing() {}

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
