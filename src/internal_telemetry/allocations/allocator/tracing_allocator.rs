use std::alloc::{handle_alloc_error, GlobalAlloc, Layout};

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
    unsafe fn get_wrapped_allocation(
        &self,
        object_layout: Layout,
    ) -> (*mut usize, *mut u8, Layout) {
        // Allocate our wrapped layout and make sure the allocation succeeded.
        let (actual_layout, offset_to_object) = get_wrapped_layout(object_layout);
        let actual_ptr = self.allocator.alloc(actual_layout);
        if actual_ptr.is_null() {
            handle_alloc_error(actual_layout);
        }

        // SAFETY: We know that `actual_ptr` is at least aligned enough for casting it to `*mut usize` as the layout for
        // the allocation backing this pointer ensures the first field in the layout is `usize.
        #[allow(clippy::cast_ptr_alignment)]
        let group_id_ptr = actual_ptr.cast::<usize>();

        // SAFETY: If the allocation succeeded and `actual_ptr` is valid, then it must be valid to advance by
        // `offset_to_object` as it would land within the allocation.
        let object_ptr = actual_ptr.wrapping_add(offset_to_object);

        (group_id_ptr, object_ptr, actual_layout)
    }
}

unsafe impl<A: GlobalAlloc, T: Tracer> GlobalAlloc for GroupedTraceableAllocator<A, T> {
    #[track_caller]
    unsafe fn alloc(&self, object_layout: Layout) -> *mut u8 {
        let (group_id_ptr, object_ptr, _wrapped_layout) =
            self.get_wrapped_allocation(object_layout);
        let object_size = object_layout.size();

        try_with_suspended_allocation_group(
            #[inline(always)]
            |group_id| {
                group_id_ptr.write(group_id.as_raw());
                self.tracer.trace_allocation(object_size, group_id);
            },
        );

        object_ptr
    }

    #[track_caller]
    unsafe fn dealloc(&self, object_ptr: *mut u8, object_layout: Layout) {
        // Regenerate the wrapped layout so we know where we have to look, as the pointer we've given relates to the
        // requested layout, not the wrapped layout that was actually allocated.
        let (wrapped_layout, offset_to_object) = get_wrapped_layout(object_layout);

        // SAFETY: We only ever return pointers to the actual requested object layout, not our wrapped layout. Since
        // global allocators cannot be changed at runtime, we know that if we're here, then the given pointer, and the
        // allocation it refers to, was allocated by us. Thus, since we wrap _all_ allocations, we know that this object
        // pointer can be safely subtracted by `offset_to_object` to get back to the group ID field in our wrapper.
        let actual_ptr = object_ptr.wrapping_sub(offset_to_object);

        // SAFETY: We know that `actual_ptr` is at least aligned enough for casting it to `*mut usize` as the layout for
        // the allocation backing this pointer ensures the first field in the layout is `usize.
        #[allow(clippy::cast_ptr_alignment)]
        let raw_group_id = actual_ptr.cast::<usize>().read();

        // Deallocate before tracking, just to make sure we're reclaiming memory as soon as possible.
        self.allocator.dealloc(actual_ptr, wrapped_layout);

        let object_size = object_layout.size();
        let source_group_id = AllocationGroupId::from_raw_unchecked(raw_group_id);

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
    static HEADER_LAYOUT: Layout = Layout::new::<usize>();

    // We generate a new allocation layout that gives us a location to store the active allocation group ID ahead
    // of the requested allocation, which lets us always attempt to retrieve it on the deallocation path.
    let (actual_layout, offset_to_object) = HEADER_LAYOUT
        .extend(object_layout)
        .expect("wrapping requested layout resulted in overflow");

    (actual_layout, offset_to_object)
}
