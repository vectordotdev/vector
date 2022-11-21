use std::{
    alloc::{handle_alloc_error, GlobalAlloc, Layout},
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

impl<A: GlobalAlloc, T: Tracer> GroupedTraceableAllocator<A, T> {
    #[inline(always)]
    unsafe fn get_wrapped_allocation(&self, object_layout: Layout) -> (*mut u8, *mut u8, Layout) {
        // Allocate our wrapped layout and make sure the allocation succeeded.
        let (actual_layout, offset_to_group_id) = get_wrapped_layout(object_layout);
        let actual_ptr = self.allocator.alloc(actual_layout);
        if actual_ptr.is_null() {
            handle_alloc_error(actual_layout);
        }

        // SAFETY: We know that `actual_ptr` with offset is at least aligned enough for casting it to `*mut u8` as the layout for
        // the allocation backing this pointer ensures the last field in the layout is `u8.
        #[allow(clippy::cast_ptr_alignment)]
        let group_id_ptr = actual_ptr.add(offset_to_group_id).cast::<u8>();

        (group_id_ptr, actual_ptr, actual_layout)
    }
}

unsafe impl<A: GlobalAlloc, T: Tracer> GlobalAlloc for GroupedTraceableAllocator<A, T> {
    #[inline]
    unsafe fn alloc(&self, object_layout: Layout) -> *mut u8 {
        let (group_id_ptr, object_ptr, _wrapped_layout) =
            self.get_wrapped_allocation(object_layout);
        let object_size = object_layout.size();
        // Group id value of zero implies allocations tracking was disabled
        // during this allocation. We override this if allocations were in fact enabled.
        group_id_ptr.write(0);
        if TRACK_ALLOCATIONS.load(Ordering::Relaxed) {
            try_with_suspended_allocation_group(
                #[inline(always)]
                |group_id| {
                    group_id_ptr.write(group_id.as_raw());
                    self.tracer.trace_allocation(object_size, group_id);
                },
            );
        }

        object_ptr
    }

    #[inline]
    unsafe fn dealloc(&self, object_ptr: *mut u8, object_layout: Layout) {
        // Regenerate the wrapped layout so we know where we have to look, as the pointer we've given relates to the
        // requested layout, not the wrapped layout that was actually allocated.
        let (wrapped_layout, offset_to_group_id) = get_wrapped_layout(object_layout);

        // SAFETY: We know that `object_ptr` with offset is at least aligned enough for casting it to `*mut u8` as the layout for
        // the allocation backing this pointer ensures the last field in the layout is `u8.
        #[allow(clippy::cast_ptr_alignment)]
        let raw_group_id = object_ptr.add(offset_to_group_id).cast::<u8>().read();

        // Deallocate before tracking, just to make sure we're reclaiming memory as soon as possible.
        self.allocator.dealloc(object_ptr, wrapped_layout);

        // Do not track deallocations when allocations weren't tracked.
        if raw_group_id == 0 {
            return;
        }

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
