use std::sync::atomic::{AtomicBool, Ordering};

mod allocator;
mod token;
mod tracer;
mod tracing;

use self::token::with_suspended_allocation_group;

pub use self::allocator::GroupedTraceableAllocator;
pub use self::token::AllocationGroupId;
pub use self::tracer::Tracer;
pub use self::tracing::AllocationLayer;

/// Whether or not allocations and deallocations should be traced.
static TRACING_ENABLED: AtomicBool = AtomicBool::new(false);

/// Enables the tracing of allocations.
pub fn enable_allocation_tracing() {
    TRACING_ENABLED.store(true, Ordering::SeqCst);
}

/// Returns `true` if allocation tracing is enabled.
pub fn is_allocation_tracing_enabled() -> bool {
    TRACING_ENABLED.load(Ordering::Relaxed)
}

/// Runs the given closure without tracing allocations or deallocations.
///
/// Inevitably, memory may need to be allocated and deallocated in the area of the program that's
/// aggregating and processing the allocator events. While `GroupedTraceableAllocator` already
/// avoids reentrantly tracing (de)allocations, this method provides a way to do so from _outside_
/// of the `GlobalAlloc` codepath.
pub fn without_allocation_tracing<F>(f: F)
where
    F: FnOnce(),
{
    with_suspended_allocation_group(f)
}
