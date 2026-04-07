use std::{
    alloc::{GlobalAlloc, Layout},
    sync::atomic::Ordering,
};

use super::{
    token::{AllocationGroupId, try_with_suspended_allocation_group},
    tracer::Tracer,
};
use crate::internal_telemetry::allocations::TRACK_ALLOCATIONS;

/// Header value for allocations made while tracking is disabled.
/// On deallocation we free with the wrapped layout but skip tracing.
const UNTRACKED: u8 = 0;

/// Header value for allocations whose tracing closure was skipped due to
/// reentrancy. `register()` never hands out `u8::MAX`, so this cannot
/// collide with a real group ID. On deallocation we free with the wrapped
/// layout but skip `trace_deallocation` to keep accounting balanced.
const UNTRACED: u8 = u8::MAX;

/// A tracing allocator that groups allocation events by groups.
///
/// Every allocation made through this allocator uses the "wrapped" layout:
/// the requested layout extended by one byte to store an allocation group
/// ID. This byte is always present regardless of whether tracking is
/// enabled, which guarantees that `dealloc` can always read a valid header
/// and free with the correct (wrapped) layout.
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
        unsafe {
            let (actual_layout, offset_to_group_id) = get_wrapped_layout(object_layout);
            let actual_ptr = self.allocator.alloc(actual_layout);
            if actual_ptr.is_null() {
                return actual_ptr;
            }

            let group_id_ptr = actual_ptr.add(offset_to_group_id).cast::<u8>();

            if !TRACK_ALLOCATIONS.load(Ordering::Relaxed) {
                group_id_ptr.write(UNTRACKED);
                return actual_ptr;
            }

            // Write the untraced sentinel so that `dealloc` always finds a
            // valid header, even when the tracing closure below is skipped
            // due to reentrancy.
            group_id_ptr.write(UNTRACED);

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
    }

    #[inline]
    unsafe fn dealloc(&self, object_ptr: *mut u8, object_layout: Layout) {
        unsafe {
            let (wrapped_layout, offset_to_group_id) = get_wrapped_layout(object_layout);
            let raw_group_id = object_ptr.add(offset_to_group_id).cast::<u8>().read();

            // Always free with the wrapped layout since all allocations
            // (tracked or not) use it.
            self.allocator.dealloc(object_ptr, wrapped_layout);

            // Skip tracing for untracked (tracking was off) and untraced
            // (reentrant, closure skipped) allocations.
            if raw_group_id == UNTRACKED || raw_group_id == UNTRACED {
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

#[cfg(test)]
mod tests {
    use std::{
        alloc::{GlobalAlloc, Layout, System},
        sync::atomic::{AtomicUsize, Ordering},
    };

    use serial_test::serial;

    use super::*;
    use crate::internal_telemetry::allocations::allocator::{
        token::AllocationGroupId, tracer::Tracer,
    };

    /// RAII guard that enables `TRACK_ALLOCATIONS` on creation and
    /// restores it to `false` on drop, ensuring cleanup even if the
    /// test panics.
    struct TrackingGuard;

    impl TrackingGuard {
        fn enable() -> Self {
            TRACK_ALLOCATIONS.store(true, Ordering::Release);
            Self
        }
    }

    impl Drop for TrackingGuard {
        fn drop(&mut self) {
            TRACK_ALLOCATIONS.store(false, Ordering::Release);
        }
    }

    struct CountingTracer {
        allocs: AtomicUsize,
        deallocs: AtomicUsize,
    }

    impl CountingTracer {
        const fn new() -> Self {
            Self {
                allocs: AtomicUsize::new(0),
                deallocs: AtomicUsize::new(0),
            }
        }
    }

    impl Tracer for CountingTracer {
        fn trace_allocation(&self, _size: usize, _group_id: AllocationGroupId) {
            self.allocs.fetch_add(1, Ordering::Relaxed);
        }

        fn trace_deallocation(&self, _size: usize, _source_group_id: AllocationGroupId) {
            self.deallocs.fetch_add(1, Ordering::Relaxed);
        }
    }

    #[test]
    fn sentinels_do_not_collide_with_root_id() {
        assert_eq!(UNTRACED, u8::MAX);
        assert_ne!(UNTRACED, AllocationGroupId::ROOT.as_raw());
        assert_ne!(UNTRACKED, AllocationGroupId::ROOT.as_raw());
    }

    /// Allocations made while tracking is disabled get UNTRACKED (0) in the
    /// header. Deallocating them (whether tracking is on or off) must use
    /// the wrapped layout and skip tracing.
    #[test]
    #[serial]
    fn untracked_alloc_dealloc_skips_tracing() {
        let allocator = GroupedTraceableAllocator::new(System, CountingTracer::new());
        let layout = Layout::from_size_align(64, 8).unwrap();

        // Tracking starts off (default state). Allocate while disabled.
        let ptr = unsafe { allocator.alloc(layout) };
        assert!(!ptr.is_null());

        // Header must be UNTRACKED.
        let (_, offset) = get_wrapped_layout(layout);
        let raw_id = unsafe { ptr.add(offset).cast::<u8>().read() };
        assert_eq!(raw_id, UNTRACKED);

        // Enable tracking, then dealloc -- must not panic, no trace events.
        let _guard = TrackingGuard::enable();
        unsafe { allocator.dealloc(ptr, layout) };

        assert_eq!(allocator.tracer.allocs.load(Ordering::Relaxed), 0);
        assert_eq!(allocator.tracer.deallocs.load(Ordering::Relaxed), 0);
    }

    /// Tracked allocation: header is a valid non-zero, non-sentinel group
    /// ID, tracing fires for both alloc and dealloc, and dealloc completes
    /// without panic.
    #[test]
    #[serial]
    fn tracked_alloc_dealloc_does_not_panic() {
        let allocator = GroupedTraceableAllocator::new(System, CountingTracer::new());
        let layout = Layout::from_size_align(64, 8).unwrap();

        let _guard = TrackingGuard::enable();
        let ptr = unsafe { allocator.alloc(layout) };
        assert!(!ptr.is_null());

        let (_, offset) = get_wrapped_layout(layout);
        let raw_id = unsafe { ptr.add(offset).cast::<u8>().read() };
        assert_eq!(
            raw_id,
            AllocationGroupId::ROOT.as_raw(),
            "header must be the ROOT group ID"
        );
        assert_eq!(allocator.tracer.allocs.load(Ordering::Relaxed), 1);

        unsafe { allocator.dealloc(ptr, layout) };

        assert_eq!(allocator.tracer.deallocs.load(Ordering::Relaxed), 1);
    }

    /// End-to-end reentrant allocation: hold a mutable borrow on the
    /// thread-local group stack so `try_with_suspended_allocation_group`
    /// skips the tracing closure. The header must be UNTRACED and both
    /// trace counters must stay at zero through alloc + dealloc.
    #[test]
    #[serial]
    fn reentrant_alloc_writes_untraced_and_skips_tracing() {
        use crate::internal_telemetry::allocations::allocator::token::LOCAL_ALLOCATION_GROUP_STACK;

        let allocator = GroupedTraceableAllocator::new(System, CountingTracer::new());
        let layout = Layout::from_size_align(64, 8).unwrap();

        let _guard = TrackingGuard::enable();

        // Hold a mutable borrow to simulate reentrancy -- any nested
        // `try_borrow_mut` inside `try_with_suspended_allocation_group`
        // will fail, causing the tracing closure to be skipped.
        LOCAL_ALLOCATION_GROUP_STACK.with(|group_stack| {
            let _borrow = group_stack.borrow_mut();

            let ptr = unsafe { allocator.alloc(layout) };
            assert!(!ptr.is_null());

            let (_, offset) = get_wrapped_layout(layout);
            let raw_id = unsafe { ptr.add(offset).cast::<u8>().read() };
            assert_eq!(
                raw_id, UNTRACED,
                "reentrant alloc must write UNTRACED sentinel"
            );

            assert_eq!(allocator.tracer.allocs.load(Ordering::Relaxed), 0);

            unsafe { allocator.dealloc(ptr, layout) };

            assert_eq!(allocator.tracer.deallocs.load(Ordering::Relaxed), 0);
        });
    }
}
