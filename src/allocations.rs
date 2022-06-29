//! Allocation tracking exposed via internal telemetry.

// TODO: We need to make the late registration thread update group entries after it registers them in `metrics`,
// otherwise we risk futures changes -- such as switching to generational storage -- breaking how these metrics are
// collected if we're not going through the normal `Counter` interface.
//
// TODO: We don't currently register the root allocation group which means we're missing all the allocations that happen
// outside of the various component tasks. Additionally, we are likely missing span propagation for spawned tasks that
// occur under component tasks. Likely, not for sure... but likely.
//
// TODO: Another big chunk of memory use at low load will be thread stacks, especially with how big/deep futures
// generators are. Thread stacks can be large: 8MB by default on Linux. This means that if we use a lot of blocking
// tasks, we're at least making calls for 8MB chunks for each thread, which is still virtual memory so it's not actually
// physically wired in all at once... but it's a large potential lever on memory usage, again, with how big/deep futures
// generators are.
//
// This is made harder to track because, for example, on Linux, `pthread_create` is using `mmap -- a syscall -- to get a
// private anonymous region for a thread's stack, so we can't even capture that allocation in our user-mode global
// allocator.
//
// TODO: Maybe we should track VSZ/RSS overall for the process so that we can at least emit it alongside the allocation
// metrics to have more of a full picture.. as you could intuit from the above TODOs, the numbers may still diverge
// quite a bit but they should all be correlated/directional enough to tell the full story.
//
// TODO: Could we take a reference to the span that we want to attach the allocation group token to, and then visit all
// of the fields to automatically extract the relevant metric tags? We could then also attach the token to the span for
// the caller, so that they never even needed to bother doing that. This would be cleaner than having to generate the
// vector of tags by hand, which is obviously a "do it once and then never change it" thing but would look a lot cleaner
// in this proposed method.
use std::{
    sync::atomic::{AtomicUsize, Ordering, AtomicBool}, collections::HashMap, mem::{MaybeUninit, self}, num::NonZeroUsize,
};

use thread_local::ThreadLocal;
use tracking_allocator::{
    AllocationGroupId, AllocationGroupToken, AllocationRegistry, AllocationTracker,
};

const POINTER_WIDTH: u32 = usize::BITS;
const PAGE_COUNT: usize = (POINTER_WIDTH + 1) as usize;

macro_rules! get_page_slots {

}

/// Snapshot of the statistics for an allocation group.
struct GroupStatisticsSnapshot {
    allocated_bytes: usize,
    deallocated_bytes: usize,
    allocations: usize,
    deallocations: usize,
}

impl GroupStatisticsSnapshot {
    fn merge(&mut self, other: GroupStatisticsSnapshot) {
        self.allocated_bytes += other.allocated_bytes;
        self.allocations += other.allocations;
        self.deallocated_bytes += other.deallocated_bytes;
        self.deallocations += other.deallocations;
    }
}

/// Statistics for an allocation group.
struct GroupStatistics {
    allocated_bytes: AtomicUsize,
    deallocated_bytes: AtomicUsize,
    allocations: AtomicUsize,
    deallocations: AtomicUsize,
}

impl GroupStatistics {
    /// Creates a new `GroupStatistics`.
    const fn new() -> Self {
        Self {
            allocated_bytes: AtomicUsize::new(0),
            deallocated_bytes: AtomicUsize::new(0),
            allocations: AtomicUsize::new(0),
            deallocations: AtomicUsize::new(0),
        }
    }

    /// Tracks an allocation.
    fn track_allocation(&self, bytes: usize) {
        self.allocated_bytes.fetch_add(bytes, Ordering::Relaxed);
        self.allocations.fetch_add(1, Ordering::Relaxed);
    }

    /// Tracks a deallocation.
    fn track_deallocation(&self, bytes: usize) {
        self.deallocated_bytes.fetch_add(bytes, Ordering::Relaxed);
        self.deallocations.fetch_sub(1, Ordering::Relaxed);
    }

    /// Collects the current statistics values, resetting the counters back to zero.
    fn collect(&self) -> GroupStatisticsSnapshot {
        GroupStatisticsSnapshot {
            allocated_bytes: self.allocated_bytes.swap(0, Ordering::Relaxed),
            allocations: self.allocations.swap(0, Ordering::Relaxed),
            deallocated_bytes: self.deallocated_bytes.swap(0, Ordering::Relaxed),
            deallocations: self.deallocations.swap(0, Ordering::Relaxed),
        }
    }
}

/// A power-of-two-sized slice of group statistics.
#[derive(Debug)]
struct GroupStatisticsStoragePage {
    page_size: usize,
    initialized: AtomicBool,
    slots: MaybeUninit<Box<[GroupStatistics]>>,
}

impl GroupStatisticsStoragePage {
    /// Creates a new `GroupStatisticsStoragePage` in an uninitialized state.
    ///
    /// Callers must initialize the underlying storage by calling `initialize`, which will allocate enough storage to
    /// store `N` elements, where `N` is equal to `2^page_exp`.
    const fn new(page_size: usize) -> Self {
        Self {
            page_size,
            initialized: AtomicBool::new(false),
            slots: MaybeUninit::uninit(),
        }
    }

    /// Gets whether or not this page has been initialized yet.
    fn is_initialized(&self) -> bool {
        self.initialized.load(Ordering::Release)
    }

    /// Initializes the page, allocating the necessary underlying storage.
    fn initialize(&self) {
        // Allocate the underlying storage for this page.
        let mut slots = Vec::with_capacity(self.page_size);
        slots.resize_with(self.page_size, GroupStatistics::new);

        // Convert our storage and take ownership of it, and mark ourselves as initialized and ready for business.
        self.slots.write(slots.into_boxed_slice());
        self.initialized.store(true, Ordering::Release);
    }

    /// Gets a reference to the given slot.
    ///
    /// # Safety
    /// 
    /// It is the callers responsibility to ensure that they have a valid index for this page. This is given by passing
    /// the regular group ID into `id_to_page`, where the page index and page subindex are given. A given page subindex
    /// is only valid for the page index it was given with.
    ///
    /// Using any other values are instant UB, and will likely cause the process to abort.
    unsafe fn get_slot_unchecked(&self, index: usize) -> &GroupStatistics {
        self.slots.assume_init_ref().get_unchecked(index)
    }

    /// Gets a reference to all slots in the page.
    ///
    /// If the page has not yet been initialized (via `initialize`), then an empty slice is returned,
    fn slots(&self) -> &[GroupStatistics] {
        if self.initialized.load(Ordering::Relaxed) {
            // SAFETY: We know that if `self.initialized` is `true`, then `initialize` has run and initialized `self.slots`.
            unsafe { self.slots.assume_init_ref() }
        } else {
            &[]
        }
    }
}

impl Drop for GroupStatisticsStoragePage {
    fn drop(&mut self) {
        if *self.initialized.get_mut() {
            // SAFETY: We know that if `self.initialized` is `true`, then `initialize` has run and initialized `self.slots`.
            unsafe { self.slots.assume_init_drop() };
        }
    }
}

struct GroupStatisticsStorage {
    page_init_mask: AtomicUsize,
    pages: [GroupStatisticsStoragePage; PAGE_COUNT]
}

impl GroupStatisticsStorage {
    fn new() -> Self {
        let mut maybe_pages: [MaybeUninit<GroupStatisticsStoragePage>; PAGE_COUNT] = unsafe {
            MaybeUninit::uninit().assume_init()
        };

        let mut page_idx: u32 = 0;
        for page in &mut maybe_pages[..] {
            let page_size = 2usize.pow(page_idx);
            page.write(GroupStatisticsStoragePage::new(page_size));
            page_idx += 1;
        }

        let pages = unsafe { mem::transmute::<_, [GroupStatisticsStoragePage; PAGE_COUNT]>(maybe_pages) };

        Self {
            page_init_mask: AtomicUsize::new(0),
            pages,
        }
    }

    /// Visits all group statistics present in storage.
    ///
    /// Callers must use `GroupStatistics::collect` to determine what has changed since the last visit, as all groups
    /// present in storage will be visited each time `visit` is called.
    fn visit<F>(&self, f: F)
    where
        F: Fn(NonZeroUsize, &GroupStatistics)
    {}

    fn try_claim_page_init(&self, page_idx: usize) -> bool {
        // Try and mark the bit that corresponds to the page index.
        let page_idx_bit = 1 << page_idx;
        let previous_page_init_mask = self.page_init_mask.fetch_or(page_idx_bit, Ordering::AcqRel);

        // If the bit wasn't already set, then this call has claimed the right to initialize the page.
        previous_page_init_mask & page_idx_bit == 0
    }

    fn register_id(&self, id: NonZeroUsize) {
        let (page_idx, _) = id_to_page(id);

        // Page initialization can happen concurrently, so we need to protect it. This means that one concurrent caller
        // will win the right to initialize the page, while others will wait until it is marked initialized.
        //
        // SAFETY: `page` can never be a value greater than `PAGE_COUNT`.
        let page = unsafe { self.pages.get_unchecked(page_idx) };
        if self.try_claim_page_init(page_idx) {
            page.initialize();
        } else {
            // Wait for the page to be initialized.
            unsafe { while !page.is_initialized() {} }
        }
    }

    /// Gets a reference to the group statistics for given ID.
    ///
    /// # Safety
    ///
    /// This function assumes that the page where the given ID lives has been previously initialized via `register_id`.
    /// Otherwise, this call will trigger instant UB, and will likely cause the process to abort.
    unsafe fn get_statistics(&self, id: NonZeroUsize) -> &GroupStatistics {
        let (page_idx, page_subidx) = id_to_page(id);
        let page = self.pages.get_unchecked(page_idx);
        page.get_slot_unchecked(page_subidx)
    }
}

#[inline]
const fn id_to_page(id: NonZeroUsize) -> (usize, usize) {
    let page = POINTER_WIDTH - id.get().leading_zeros();
    let page_size = 1 << page.saturating_sub(1);
    let idx = id.get() ^ page_size;

    // SAFETY: We can blindly cast to `usize` as both `POINTER_WIDTH` and `leading_zeros` will only ever return values
    // that track the number of bits in a pointer, and it is impossible for `usize` to not be able to hold a number
    // describing its own bit length.
    (page as usize, idx)
}

struct Collector {
    statistics: &'static ThreadLocal<GroupStatisticsStorage>,
}

impl Collector {
    fn collect_statistics(&self) -> HashMap<usize, GroupStatisticsSnapshot> {
        let mut groups = HashMap::new();

        for local_groups in self.statistics.iter() {

        }
    }
}

struct Tracker {
    statistics: &'static ThreadLocal<GroupStatisticsStorage>,
}

impl Tracker {
    pub fn new() -> (Self, Collector) {
        let statistics = Box::leak(Box::new(ThreadLocal::new()));

        let tracker = Self { statistics };
        let collector = Collector { statistics };

        (tracker, collector)
    }

    fn get_group_stats(&self, group_id: AllocationGroupId) -> &GroupStatistics {
        let group_id_idx = group_id.as_usize().get();
        let local_stats = self.statistics.get_or(|| UnsafeCell::new(Vec::new()));

        // Make sure our list of group statistics for this thread is already allocated up to a point where we can
        // directly index into `self.statistics` to get a reference to it in the future.
        //
        // SAFETY: We have exclusive access to the `UnsafeCell<...>` as it is `!Sync`, so we can create a mutable
        // reference to the contained `Vec<GroupStatistics>` as it will be the only mutable reference.
        {
            unsafe {
                let local_stats_mut = &mut *local_stats.get();
                if local_stats_mut.len() < group_id_idx {
                    local_stats_mut.resize_with(group_id_idx - 1, || GroupStatistics::new());
                }
            }
        }

        // SAFETY: We know that the vector is sized large enough, and with an initialized value, to have a statistics
        // entry at the index `group_id_idx` based on the prior call to `Vec::resize_with`.
        unsafe { (&*local_stats.get()).get_unchecked(group_id_idx) }
    }
}

impl AllocationTracker for Tracker {
    fn allocated(&self, _addr: usize, size: usize, group_id: AllocationGroupId) {
        let local_stats_group = self.get_group_stats(group_id);
        local_stats_group.track_allocation(size);
    }

    fn deallocated(
        &self,
        _addr: usize,
        size: usize,
        source_group_id: AllocationGroupId,
        _current_group_id: AllocationGroupId,
    ) {
        let local_stats_group = self.get_group_stats(source_group_id);
        local_stats_group.track_deallocation(size);
    }
}

/// Initializes allocation tracking.
///
/// This sets the global allocation tracker to our custom tracker implementation, spawns a background thread which
/// handles registering allocation groups by attaching their atomic counters to our internal metrics backend, and then
/// finally enables tracking which causes (de)allocation events to begin flowing.
pub fn init_allocation_tracking() {
    let (tracker, _collector) = Tracker::new();

    let _ = AllocationRegistry::set_global_tracker(tracker)
        .expect("no other global tracker should be set yet");

    AllocationRegistry::enable_tracking();
}

/// Acquires an allocation group token.
///
/// This creates an allocation group which allows callers to enter/exit the allocation group context, associating all
/// (de)allocations within the context with that group.  That token can (and typically is) associated with a
/// /// `tracing::Span` such that the context is entered and exited as the span is entered and exited. This allows
/// ensuring that we track all (de)allocations when the span is active.
///
/// # Tags
///
/// The provided `tags` are used for the metrics that get registered and attached to the allocation group. No tags from
/// the traditional `metrics`/`tracing` are collected, as the metrics are updated directly rather than emitted via the
/// traditional `metrics` macros, so the given tags should match the span fields that would traditionally be set for a
/// given span in order to ensure that they match.
pub fn acquire_allocation_group_token(_tags: Vec<(String, String)>) -> AllocationGroupToken {
    AllocationGroupToken::register().expect("failed to register allocation group token")
}
