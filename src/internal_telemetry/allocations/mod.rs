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
use std::sync::atomic::{AtomicUsize, Ordering};

use once_cell::sync::OnceCell;
use thread_local::ThreadLocal;
use tracking_allocator::{
    AllocationGroupId, AllocationGroupToken, AllocationRegistry, AllocationTracker,
};

mod storage;
use storage::PageTable;

static COLLECTOR: OnceCell<Collector> = OnceCell::new();

fn get_global_collector() -> &'static Collector {
    COLLECTOR.get_or_init(|| {
        // Create the collector, but more importantly, register the root allocation group as we must ensure it exists
        // before enabling tracking, as its the only group that can exist prior to tracking being enabled, and that
        // doesn't get registered through our own allocation group token registration flow, which is where we would
        // otherwise register group IDs in the page table.
        let collector = Collector::new();
        //collector.register(AllocationGroupId::ROOT);
        collector
    })
}

/// Snapshot of the statistics for an allocation group.
/*struct GroupStatisticsSnapshot {
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
}*/

/// Statistics for an allocation group.
#[derive(Default)]
struct GroupStatistics {
    allocated_bytes: AtomicUsize,
    deallocated_bytes: AtomicUsize,
    allocations: AtomicUsize,
    deallocations: AtomicUsize,
}

impl GroupStatistics {
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

    /*
    /// Collects the current statistics values, resetting the counters back to zero.
    fn collect(&self) -> GroupStatisticsSnapshot {
        GroupStatisticsSnapshot {
            allocated_bytes: self.allocated_bytes.swap(0, Ordering::Relaxed),
            allocations: self.allocations.swap(0, Ordering::Relaxed),
            deallocated_bytes: self.deallocated_bytes.swap(0, Ordering::Relaxed),
            deallocations: self.deallocations.swap(0, Ordering::Relaxed),
        }
    }*/
}

struct Collector {
    statistics: &'static ThreadLocal<PageTable<GroupStatistics>>,
}

impl Collector {
    fn new() -> Self {
        let statistics = Box::leak(Box::new(ThreadLocal::new()));

        Self { statistics }
    }

    fn get_tracker(&self) -> Tracker {
        Tracker {
            statistics: self.statistics,
        }
    }

    /*
    fn register(&self, group_id: AllocationGroupId) {
        let local_stats_table = self.statistics.get_or_default();
        local_stats_table.register(group_id.as_usize().get())
    }

    fn collect_statistics(&self) -> HashMap<usize, GroupStatisticsSnapshot> {
        let mut groups = HashMap::new();

        for local_groups in self.statistics.iter() {}

        groups
    }*/
}

struct Tracker {
    statistics: &'static ThreadLocal<PageTable<GroupStatistics>>,
}

impl Tracker {
    fn get_local_group_stats(&self, group_id: AllocationGroupId) -> &GroupStatistics {
        let local_stats_table = self.statistics.get_or_default();

        // SAFETY: In order for calls to `get` to be safe, the group ID we pass must have been previously registered
        // (via `register`) otherwise we will instantaneously trigger UB. As this method overall can only be called by
        // the global allocator, it implies that we're inside an allocation group, and allocation groups can only be
        // entered after acquiring a token via `register_allocation_group_token`, which the group ID is derived from.
        //
        // Thus, we cannot be here without the given group ID having already been registered correctly.
        unsafe { local_stats_table.get(group_id.as_usize().get()) }
    }
}

impl AllocationTracker for Tracker {
    fn allocated(&self, _addr: usize, size: usize, group_id: AllocationGroupId) {
        let local_group_stats = self.get_local_group_stats(group_id.clone());
        //println!("tid {:?}: got group stats for {}", std::thread::current().id(), group_id.as_usize().get());
        local_group_stats.track_allocation(size);
    }

    fn deallocated(
        &self,
        _addr: usize,
        size: usize,
        source_group_id: AllocationGroupId,
        _current_group_id: AllocationGroupId,
    ) {
        let local_group_stats = self.get_local_group_stats(source_group_id);
        local_group_stats.track_deallocation(size);
    }
}

/// Initializes allocation tracking.
pub fn init_allocation_tracking() {
    let collector = get_global_collector();
    let tracker = collector.get_tracker();

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
    let token =
        AllocationGroupToken::register().expect("failed to register allocation group token");

    //let collector = get_global_collector();
    //collector.register(token.id());

    // BROKEN: currently, we register a token on the thread it's created, which initializes the page it needs, etc
    // etc... but what we actually need to do is initialize it in all threads, or know that it isn't already initialized
    // in the current thread and do so... but that implies tracking a lot of per-thread state which might get squicky,
    // and would mean a runtime check per tracked allocation, which kinda sucks...
    //
    // this might mean that we actually want to do at least the `is_initialized`/`initialize` call automatically in
    // `LazilyAllocatedPage::get` if we can make sure it's super cheap, and then we can make that method much safer

    token
}
