mod allocator;
mod group;
mod tracing;

use std::{cell::RefCell, sync::atomic::{AtomicBool, AtomicU64, Ordering}, thread, time::Duration};

pub use self::allocator::GroupedTraceableAllocator;
pub use self::group::*;
pub use self::tracing::AllocationLayer;

thread_local! {
    /// A stack representing the currently active allocation groups.
    ///
    /// As an allocation group is entered and exited, it will be pushed and popped from the group
    /// stack. Any allocations which occur on this thread will be associated with whichever
    /// allocation group is currently at the top of the stack at the time that the allocation
    /// occurs.
    static ALLOCATION_GROUP_STACK: RefCell<GroupStack<256>> =
        const { RefCell::new(GroupStack::new()) };
}

// Whether or not to trace (de)allocations.
pub static TRACE_ALLOCATIONS: AtomicBool = AtomicBool::new(false);

// Reporting interval of (de)allocation statistics, in milliseconds.
pub static REPORTING_INTERVAL_MS: AtomicU64 = AtomicU64::new(5000);

/// Initializes allocation tracing.
pub fn init_allocation_tracing_reporter() {
    let alloc_processor = thread::Builder::new().name("vector-alloc-processor".to_string());
    alloc_processor
        .spawn(|| {
            without_allocation_tracing(|| loop {
                thread::sleep(Duration::from_millis(
                    REPORTING_INTERVAL_MS.load(Ordering::Relaxed),
                ));
            })
        })
        .unwrap();
}

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
