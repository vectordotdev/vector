use std::{
    alloc::{GlobalAlloc, Layout},
    sync::atomic::Ordering,
};

use crate::internal_telemetry::allocations::TRACE_ALLOCATIONS;

use super::{accumulator::Accumulator, get_current_allocation_group, AllocationGroup};

/// A tracing allocator that groups allocation events by groups.
///
/// This allocator can only be used when specified via `#[global_allocator]`.
pub struct GroupedTracingAllocator<A> {
    allocator: A,
}

impl<A> GroupedTracingAllocator<A> {
    /// Creates a new `GroupedTracingAllocator` that wraps the given allocator.
    #[must_use]
    pub const fn new(allocator: A) -> Self {
        Self { allocator }
    }
}

unsafe impl<A: GlobalAlloc> GlobalAlloc for GroupedTracingAllocator<A> {
    #[inline]
    unsafe fn alloc(&self, object_layout: Layout) -> *mut u8 {
        // Don't trace allocations unless enabled.
        if !TRACE_ALLOCATIONS.load(Ordering::Relaxed) {
            return self.allocator.alloc(object_layout);
        }

        // Wrap the requested allocation so that we have a slot to store the allocation group
        // reference at the end of the object, which we'll read during deallocation to figure out
        // who owns this allocation.
        let (actual_layout, offset_to_group_ref) = get_wrapped_layout(object_layout);
        let actual_ptr = self.allocator.alloc(actual_layout);
        if actual_ptr.is_null() {
            return actual_ptr;
        }

        // Write the reference to the active allocation group at the end of the allocation so we can
        // access it when this allocation is, eventually, deallocated.
        let allocation_group_ref = get_current_allocation_group();
        let group_ref_ptr = actual_ptr
            .add(offset_to_group_ref)
            .cast::<&'static AllocationGroup>();
        group_ref_ptr.write(allocation_group_ref);

        Accumulator::track_allocation_local(actual_layout.size() as u64);

        actual_ptr
    }

    #[inline]
    unsafe fn dealloc(&self, object_ptr: *mut u8, object_layout: Layout) {
        // Don't trace deallocations unless enabled.
        if !TRACE_ALLOCATIONS.load(Ordering::Relaxed) {
            self.allocator.dealloc(object_ptr, object_layout);
            return;
        }

        // Regenerate the wrapped layout so we know where we have to look, as the pointer we've
        // given relates to the requested layout, not the wrapped layout that was actually
        // allocated.
        //
        // Once we have that, we can reconstitute the reference to the allocation group that the
        // allocation belongs to.
        let (wrapped_layout, offset_to_group_ref) = get_wrapped_layout(object_layout);
        let allocation_group_ref = object_ptr
            .add(offset_to_group_ref)
            .cast::<&'static AllocationGroup>()
            .read();

        // Deallocate before tracking, just to make sure we're reclaiming memory as soon as possible.
        self.allocator.dealloc(object_ptr, wrapped_layout);

        allocation_group_ref.track_deallocation(wrapped_layout.size() as u64);
    }
}

#[inline(always)]
fn get_wrapped_layout(object_layout: Layout) -> (Layout, usize) {
    static TRAILER_LAYOUT: Layout = Layout::new::<&'static AllocationGroup>();

    // We generate a new allocation layout that gives us a location to store the active allocation
    // group reference behind the requested allocation, which lets us always attempt to retrieve it
    // on the deallocation path.
    let (actual_layout, offset_to_group_ref) = object_layout
        .extend(TRAILER_LAYOUT)
        .expect("wrapping requested layout resulted in overflow");

    (actual_layout.pad_to_align(), offset_to_group_ref)
}
