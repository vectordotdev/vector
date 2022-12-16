use std::{
    alloc::{GlobalAlloc, Layout},
    sync::atomic::Ordering,
};

use crate::internal_telemetry::allocations::TRACK_ALLOCATIONS;

use super::{
    token::{try_with_suspended_allocation_group, AllocationGroupId},
    tracer::Tracer,
};

/// A tracing allocator that groups allocation events by groups.
///
/// This allocator can only be used when specified via `#[global_allocator]`.
pub struct GroupedTraceableAllocator<A, T> {
    allocator: A,
    tracer: T,
}

impl<A, T> GroupedTraceableAllocator<A, T> {
    /// Creates a new `GroupedTraceableAllocator` that wraps the given allocator and tracer.
    #[must_use]
    pub const fn new(allocator: A, tracer: T) -> Self {
        Self { allocator, tracer }
    }
}

unsafe impl<A: GlobalAlloc, T: Tracer> GlobalAlloc for GroupedTraceableAllocator<A, T> {
    #[inline]
    unsafe fn alloc(&self, object_layout: Layout) -> *mut u8 {
        if !TRACK_ALLOCATIONS.load(Ordering::Relaxed) {
            return self.allocator.alloc(object_layout);
        }

        // Allocate our wrapped layout and make sure the allocation succeeded.
        let (actual_layout, offset_to_group_id) = get_wrapped_layout(object_layout);
        let actual_ptr = self.allocator.alloc(actual_layout);
        if actual_ptr.is_null() {
            return actual_ptr;
        }

        let group_id_ptr = actual_ptr.add(offset_to_group_id).cast::<u8>();

        let object_size = object_layout.size();

        try_with_suspended_allocation_group(
            #[inline(always)]
            |group_id| {
                group_id_ptr.write(group_id.as_raw());
                self.tracer.trace_allocation(object_size, group_id);
            },
        );
        actual_ptr
    }

    #[inline]
    unsafe fn dealloc(&self, object_ptr: *mut u8, object_layout: Layout) {
        if !TRACK_ALLOCATIONS.load(Ordering::Relaxed) {
            self.allocator.dealloc(object_ptr, object_layout);
            return;
        }
        // Regenerate the wrapped layout so we know where we have to look, as the pointer we've given relates to the
        // requested layout, not the wrapped layout that was actually allocated.
        let (wrapped_layout, offset_to_group_id) = get_wrapped_layout(object_layout);

        let raw_group_id = object_ptr.add(offset_to_group_id).cast::<u8>().read();

        // Deallocate before tracking, just to make sure we're reclaiming memory as soon as possible.
        self.allocator.dealloc(object_ptr, wrapped_layout);

        let object_size = object_layout.size();
        let source_group_id = AllocationGroupId::from_raw(raw_group_id);

        try_with_suspended_allocation_group(
            #[inline(always)]
            |_| {
                self.tracer.trace_deallocation(object_size, source_group_id);
            },
        );
    }
}

#[inline(always)]
fn get_wrapped_layout(object_layout: Layout) -> (Layout, usize) {
    static HEADER_LAYOUT: Layout = Layout::new::<u8>();

    // We generate a new allocation layout that gives us a location to store the active allocation group ID ahead
    // of the requested allocation, which lets us always attempt to retrieve it on the deallocation path.
    let (actual_layout, offset_to_group_id) = object_layout
        .extend(HEADER_LAYOUT)
        .expect("wrapping requested layout resulted in overflow");

    (actual_layout.pad_to_align(), offset_to_group_id)
}
