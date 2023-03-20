use std::{
    alloc::{GlobalAlloc, Layout},
    cmp, ptr,
    sync::atomic::Ordering,
};

use crate::internal_telemetry::allocations::TRACE_ALLOCATIONS;

use super::{accumulator::Accumulator, get_current_allocation_group, AllocationGroup};

const TRAILER_CANARY: u64 = 0xFEEDFACECAFEBEEF;

struct MetadataTrailer {
    /// Canary value, used to detect whether or not this metadata trailer was pulled from an
    /// allocation that was traced (and has a valid group reference) or not.
    canary: u64,

    /// The allocation group which owns the allocation.
    group_ref: &'static AllocationGroup,
}

impl MetadataTrailer {
    /// Creates a `MetadataTrailer` from the given allocation group.
    const fn from_group_ref(group_ref: &'static AllocationGroup) -> Self {
        Self {
            canary: TRAILER_CANARY,
            group_ref,
        }
    }

    /// Whether or not this metadata is valid based on the canary.
    const fn is_valid(&self) -> bool {
        self.canary == TRAILER_CANARY
    }

    /// Tries to get a reference to the allocation group represented by this allocation metadata.
    ///
    /// If the canary is not valid, `None` is returned. Otherwise, `Some(...)` is returned
    /// containing a reference to the allocation group.
    const fn try_group(&self) -> Option<&'static AllocationGroup> {
        // The canary value is meant to be sufficiently unique that if we can read the canary value
        // from the field, we can be sure that this `MetadataTrailer` was read from a real
        // allocation that was traced, and that the resulting group reference is also valid.
        //
        // If the canary value is not the right one, then we return `None`, which signifies that
        // this was (most likely) an allocation that occurred prior to allocation tracing being
        // enabled.
        if self.is_valid() {
            Some(self.group_ref)
        } else {
            None
        }
    }
}

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
    unsafe fn alloc(&self, object_layout: Layout) -> *mut u8 {
        // Don't trace allocations unless enabled.
        if !TRACE_ALLOCATIONS.load(Ordering::Relaxed) {
            return self.allocator.alloc(object_layout);
        }

        // Wrap the requested allocation so that we have a slot to store the metadata trailer at the
        // end of the object, and then populate it.
        let (actual_layout, offset_to_trailer) = get_wrapped_layout(object_layout);
        let actual_ptr = self.allocator.alloc(actual_layout);
        if actual_ptr.is_null() {
            return actual_ptr;
        }

        let metadata = MetadataTrailer::from_group_ref(get_current_allocation_group());
        let trailer_ptr = actual_ptr.add(offset_to_trailer).cast::<MetadataTrailer>();
        trailer_ptr.write(metadata);

        // Track the allocation.
        Accumulator::track_allocation_local(actual_layout.size() as u64);

        actual_ptr
    }

    unsafe fn dealloc(&self, object_ptr: *mut u8, object_layout: Layout) {
        // Don't trace deallocations unless enabled.
        if !TRACE_ALLOCATIONS.load(Ordering::Relaxed) {
            self.allocator.dealloc(object_ptr, object_layout);
            return;
        }

        // Regenerate the wrapped layout so we can attempt to read the metadata trailer.
        //
        // If this allocation was traced initially, then we should be able to read the trailer and
        // get a reference to the allocation group which owns the allocation. If we get back a valid
        // group reference, we'll track the deallocation.
        //
        // Not all allocations will have an allocation group attached to them, specifically any
        // allocations that happened before allocation tracing was enabled.
        let (wrapped_layout, offset_to_trailer) = get_wrapped_layout(object_layout);
        let metadata = object_ptr
            .add(offset_to_trailer)
            .cast::<MetadataTrailer>()
            .read();

        if let Some(group) = metadata.try_group() {
            group.track_deallocation(wrapped_layout.size() as u64);
        }

        // Deallocate the object.
        self.allocator.dealloc(object_ptr, wrapped_layout);
    }

    unsafe fn realloc(
        &self,
        old_object_ptr: *mut u8,
        old_object_layout: Layout,
        new_object_size: usize,
    ) -> *mut u8 {
        // Don't trace reallocations unless enabled.
        if !TRACE_ALLOCATIONS.load(Ordering::Relaxed) {
            return self
                .allocator
                .realloc(old_object_ptr, old_object_layout, new_object_size);
        }

        // Regenerate the wrapped layout so we can attempt to read the metadata trailer.
        //
        // If this allocation was traced initially, then we should be able to read the trailer and
        // get a reference to the allocation group which owns the allocation. If we get back a valid
        // group reference, we'll track the deallocation.
        //
        // Not all allocations will have an allocation group attached to them, specifically any
        // allocations that happened before allocation tracing was enabled.
        let (_, offset_to_trailer) = get_wrapped_layout(old_object_layout);
        let metadata = old_object_ptr
            .add(offset_to_trailer)
            .cast::<MetadataTrailer>()
            .read();

        // Calculate the new baseline layout for the underlying object.
        //
        // SAFETY: The caller must ensure that the `object_new_size` does not overflow.
        // `object_layout.align()` comes from a `Layout` and is thus guaranteed to be valid.
        let new_object_layout = unsafe {
            Layout::from_size_align_unchecked(new_object_size, old_object_layout.align())
        };

        // If the metadata is valid, then allocate the new object with a wrapped layout so we can
        // transfer over the metadata trailer. Otherwise, allocate the object unchanged via the
        // underlying allocator.
        let new_object_ptr = if metadata.is_valid() {
            // Wrap the new requested allocation so that we have a slot to store the metadata
            // trailer at the end of the object, and then populate it with the existing metadata
            // trailer.
            let (new_actual_layout, offset_to_trailer) = get_wrapped_layout(new_object_layout);
            let new_object_ptr = self.allocator.alloc(new_actual_layout);
            if !new_object_ptr.is_null() {
                let trailer_ptr = new_object_ptr
                    .add(offset_to_trailer)
                    .cast::<MetadataTrailer>();
                trailer_ptr.write(metadata);

                Accumulator::track_allocation_local(new_actual_layout.size() as u64);
            }

            new_object_ptr
        } else {
            self.allocator.alloc(new_object_layout)
        };

        if new_object_ptr.is_null() {
            return new_object_ptr;
        }

        // Since we've successfully acquired the newly-sized allocation, regardless of whether or
        // not the metadata was present, we can go ahead and copy the old object value and then
        // deallocate the original allocation.
        //
        // SAFETY: The previously allocated block cannot overlap the newly allocated block.
        // The safety contract for `dealloc` must be upheld by the caller.
        unsafe {
            ptr::copy_nonoverlapping(
                old_object_ptr,
                new_object_ptr,
                cmp::min(old_object_layout.size(), new_object_size),
            );
            self.dealloc(old_object_ptr, old_object_layout);
        }

        new_object_ptr
    }
}

fn get_wrapped_layout(object_layout: Layout) -> (Layout, usize) {
    static TRAILER_LAYOUT: Layout = Layout::new::<MetadataTrailer>();

    // We generate a new allocation layout that gives us a location to store the allocation metadata
    // at the end of the original object itself. This provides a stable location to read, during
    // deallocation, for figuring out if an allocation was traced, and if so, what allocation group
    // owns the allocation.
    let (actual_layout, offset_to_trailer) = object_layout
        .extend(TRAILER_LAYOUT)
        .expect("wrapping requested layout resulted in overflow");

    (actual_layout.pad_to_align(), offset_to_trailer)
}
