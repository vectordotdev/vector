use self::token::with_suspended_allocation_group;
mod stack;
mod token;
mod tracer;
mod tracing;
mod tracing_allocator;

pub use self::token::AllocationGroupId;
pub use self::token::AllocationGroupToken;
pub use self::tracer::Tracer;
pub use self::tracing::AllocationLayer;
pub use self::tracing_allocator::GroupedTraceableAllocator;

/// Runs the given closure without tracing allocations or deallocations.
///
/// Inevitably, memory may need to be allocated and deallocated in the area of the program that's
/// aggregating and processing the allocator events. While `GroupedTraceableAllocator` already
/// avoids reentrantly tracing (de)allocations, this method provides a way to do so from _outside_
/// of the `GlobalAlloc` codepath.
#[inline(always)]
pub fn without_allocation_tracing<F>(f: F)
where
    F: FnOnce(),
{
    with_suspended_allocation_group(f)
}
