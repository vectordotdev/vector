//! Allocation tracking exposed via internal telemetry.

mod allocator;
use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Mutex,
    },
    thread,
    time::Duration,
};

use arr_macro::arr;
use metrics::{counter, decrement_gauge, increment_gauge};
use rand_distr::num_traits::ToPrimitive;

use self::allocator::Tracer;

pub(crate) use self::allocator::{
    without_allocation_tracing, AllocationGroupId, AllocationLayer, GroupedTraceableAllocator,
};

const NUM_GROUPS: usize = 128;

// Allocations are not tracked during startup.
// We use the Relaxed ordering for both stores and loads of this atomic as no other threads exist when
// this code is running, and all future threads will have a happens-after relationship with
// this thread -- the main thread -- ensuring that they see the latest value of TRACK_ALLOCATIONS.
pub static TRACK_ALLOCATIONS: AtomicBool = AtomicBool::new(false);

pub fn is_allocation_tracing_enabled() -> bool {
    TRACK_ALLOCATIONS.load(Ordering::Acquire)
}

/// Track allocations and deallocations separately.
struct GroupMemStatsStorage {
    allocations: [AtomicU64; NUM_GROUPS],
    deallocations: [AtomicU64; NUM_GROUPS],
}

// Reporting interval in milliseconds.
pub static REPORTING_INTERVAL_MS: AtomicU64 = AtomicU64::new(5000);

/// A registry for tracking each thread's group memory statistics.
static THREAD_LOCAL_REFS: Mutex<Vec<&'static GroupMemStatsStorage>> = Mutex::new(Vec::new());

/// Group memory statistics per thread.
struct GroupMemStats {
    stats: &'static GroupMemStatsStorage,
}

impl GroupMemStats {
    /// Allocates a [`GroupMemStatsStorage`], and updates the global [`THREAD_LOCAL_REFS`] registry
    /// with a reference to this newly allocated memory.
    pub fn new() -> Self {
        let mut mutex = THREAD_LOCAL_REFS.lock().unwrap();
        let stats_ref: &'static GroupMemStatsStorage = Box::leak(Box::new(GroupMemStatsStorage {
            allocations: arr![AtomicU64::new(0) ; 128],
            deallocations: arr![AtomicU64::new(0) ; 128],
        }));
        let group_mem_stats = GroupMemStats { stats: stats_ref };
        mutex.push(stats_ref);
        group_mem_stats
    }
}

thread_local! {
    static GROUP_MEM_STATS: GroupMemStats = GroupMemStats::new();
}

struct GroupInfo {
    component_kind: String,
    component_type: String,
    component_id: String,
}

impl GroupInfo {
    const fn new() -> Self {
        Self {
            component_id: String::new(),
            component_kind: String::new(),
            component_type: String::new(),
        }
    }
}

static GROUP_INFO: [Mutex<GroupInfo>; NUM_GROUPS] = arr![Mutex::new(GroupInfo::new()); 128];

pub type Allocator<A> = GroupedTraceableAllocator<A, MainTracer>;

pub const fn get_grouped_tracing_allocator<A>(allocator: A) -> Allocator<A> {
    GroupedTraceableAllocator::new(allocator, MainTracer)
}

pub struct MainTracer;

impl Tracer for MainTracer {
    #[inline(always)]
    fn trace_allocation(&self, object_size: usize, group_id: AllocationGroupId) {
        // Handle the case when thread local destructor is ran.
        _ = GROUP_MEM_STATS.try_with(|t| {
            t.stats.allocations[group_id.as_raw() as usize]
                .fetch_add(object_size as u64, Ordering::Relaxed)
        });
    }

    #[inline(always)]
    fn trace_deallocation(&self, object_size: usize, source_group_id: AllocationGroupId) {
        // Handle the case when thread local destructor is ran.
        _ = GROUP_MEM_STATS.try_with(|t| {
            t.stats.deallocations[source_group_id.as_raw() as usize]
                .fetch_add(object_size as u64, Ordering::Relaxed)
        });
    }
}

/// Initializes allocation tracing.
pub fn init_allocation_tracing() {
    for group in &GROUP_INFO {
        let mut writer = group.lock().unwrap();
        *writer = GroupInfo {
            component_id: "root".to_string(),
            component_kind: "root".to_string(),
            component_type: "root".to_string(),
        };
    }
    let alloc_processor = thread::Builder::new().name("vector-alloc-processor".to_string());
    alloc_processor
        .spawn(|| {
            without_allocation_tracing(|| loop {
                for (group_idx, group) in GROUP_INFO.iter().enumerate() {
                    let mut allocations_diff = 0;
                    let mut deallocations_diff = 0;
                    let mutex = THREAD_LOCAL_REFS.lock().unwrap();
                    for idx in 0..mutex.len() {
                        allocations_diff +=
                            mutex[idx].allocations[group_idx].swap(0, Ordering::Relaxed);
                        deallocations_diff +=
                            mutex[idx].deallocations[group_idx].swap(0, Ordering::Relaxed);
                    }
                    if allocations_diff == 0 && deallocations_diff == 0 {
                        continue;
                    }
                    let mem_used_diff = allocations_diff as i64 - deallocations_diff as i64;
                    let group_info = group.lock().unwrap();
                    if allocations_diff > 0 {
                        counter!(
                            "component_allocated_bytes_total",
                            allocations_diff,
                            "component_kind" => group_info.component_kind.clone(),
                            "component_type" => group_info.component_type.clone(),
                            "component_id" => group_info.component_id.clone());
                    }
                    if deallocations_diff > 0 {
                        counter!(
                            "component_deallocated_bytes_total",
                            deallocations_diff,
                            "component_kind" => group_info.component_kind.clone(),
                            "component_type" => group_info.component_type.clone(),
                            "component_id" => group_info.component_id.clone());
                    }
                    if mem_used_diff > 0 {
                        increment_gauge!(
                            "component_allocated_bytes",
                            mem_used_diff.to_f64().expect("failed to convert mem_used from int to float"),
                            "component_kind" => group_info.component_kind.clone(),
                            "component_type" => group_info.component_type.clone(),
                            "component_id" => group_info.component_id.clone());
                    }
                    if mem_used_diff < 0 {
                        decrement_gauge!(
                            "component_allocated_bytes",
                            -mem_used_diff.to_f64().expect("failed to convert mem_used from int to float"),
                            "component_kind" => group_info.component_kind.clone(),
                            "component_type" => group_info.component_type.clone(),
                            "component_id" => group_info.component_id.clone());
                    }
                }
                thread::sleep(Duration::from_millis(
                    REPORTING_INTERVAL_MS.load(Ordering::Relaxed),
                ));
            })
        })
        .unwrap();
}

/// Acquires an allocation group ID.
///
/// This creates an allocation group which allows callers to enter/exit the allocation group context, associating all
/// (de)allocations within the context with that group. An allocation group ID must be "attached" to
/// a [`tracing::Span`] to achieve this" we utilize the logical invariants provided by spans --
/// entering, exiting, and how spans exist as a stack -- in order to handle keeping the "current
/// allocation group" accurate across all threads.
pub fn acquire_allocation_group_id(
    component_id: String,
    component_type: String,
    component_kind: String,
) -> AllocationGroupId {
    if let Some(group_id) = AllocationGroupId::register() {
        if let Some(group_lock) = GROUP_INFO.get(group_id.as_raw() as usize) {
            let mut writer = group_lock.lock().unwrap();
            *writer = GroupInfo {
                component_id,
                component_kind,
                component_type,
            };

            return group_id;
        }
    }

    // TODO: Technically, `NUM_GROUPS` is lower (128) than the upper bound for the
    // `AllocationGroupId::register` call itself (253), so we can hardcode `NUM_GROUPS` here knowing
    // it's the lower of the two values and will trigger first.. but this may not always be true.
    warn!("Maximum number of registrable allocation group IDs reached ({}). Allocations for component '{}' will be attributed to the root allocation group.", NUM_GROUPS, component_id);
    AllocationGroupId::ROOT
}
