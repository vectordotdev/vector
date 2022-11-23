//! Allocation tracking exposed via internal telemetry.

mod allocator;
use std::{
    sync::{
        atomic::{AtomicBool, AtomicI64, Ordering},
        Mutex,
    },
    thread,
    time::Duration,
};

use arr_macro::arr;
use metrics::gauge;
use rand_distr::num_traits::ToPrimitive;

use self::allocator::Tracer;

pub(crate) use self::allocator::{
    without_allocation_tracing, AllocationGroupId, AllocationLayer, GroupedTraceableAllocator,
};

const NUM_GROUPS: usize = 128;
pub static TRACK_ALLOCATIONS: AtomicBool = AtomicBool::new(false);

type GroupMemStatsArray = [AtomicI64; NUM_GROUPS];

/// A registry for tracking each thread's group memory statistics.
static THREAD_LOCAL_REFS: Mutex<Vec<&'static GroupMemStatsArray>> = Mutex::new(Vec::new());

/// Group memory statistics per thread.
struct GroupMemStats {
    stats: &'static GroupMemStatsArray,
}

impl GroupMemStats {
    /// Allocates a [`GroupMemStatsArray`], and updates the global [`THREAD_LOCAL_REFS`] registry
    /// with a reference to this newly allocated memory.
    pub fn new() -> Self {
        let mut mutex = THREAD_LOCAL_REFS.lock().unwrap();
        let stats_ref: &'static GroupMemStatsArray =
            Box::leak(Box::new(arr![AtomicI64::new(0) ; 128]));
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
        let _ = GROUP_MEM_STATS.try_with(|t| {
            t.stats[group_id.as_raw() as usize].fetch_add(object_size as i64, Ordering::Relaxed)
        });
    }

    #[inline(always)]
    fn trace_deallocation(&self, object_size: usize, source_group_id: AllocationGroupId) {
        // Handle the case when thread local destructor is ran.
        let _ = GROUP_MEM_STATS.try_with(|t| {
            t.stats[source_group_id.as_raw() as usize]
                .fetch_sub(object_size as i64, Ordering::Relaxed)
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
        .spawn(|| loop {
            if TRACK_ALLOCATIONS.load(Ordering::Relaxed) {
                without_allocation_tracing(|| {
                    for (group_idx, group) in GROUP_INFO.iter().enumerate() {
                        let mut mem_used = 0;
                        let mutex = THREAD_LOCAL_REFS.lock().unwrap();
                        for idx in 0..mutex.len() {
                            mem_used += mutex[idx][group_idx].load(Ordering::Relaxed);
                        }
                        if mem_used == 0 {
                            continue;
                        }
                        let group_info = group.lock().unwrap();
                        gauge!(
                        "component_allocated_bytes",
                        mem_used.to_f64().expect("failed to convert group_id from int to float"),
                        "component_kind" => group_info.component_kind.clone(),
                        "component_type" => group_info.component_type.clone(),
                        "component_id" => group_info.component_id.clone());
                    }
                });
            }
            thread::sleep(Duration::from_millis(5000));
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
    let group_id =
        AllocationGroupId::register().expect("failed to register allocation group token");
    let idx = group_id.as_raw();
    match GROUP_INFO.get(idx as usize) {
        Some(mutex) => {
            let mut writer = mutex.lock().unwrap();
            *writer = GroupInfo {
                component_id,
                component_kind,
                component_type,
            };
            group_id
        }
        None => {
            info!("Maximum number of registrable allocation group IDs reached ({}). Allocations for component '{}' will be attributed to the root allocation group.", NUM_GROUPS, component_id);
            AllocationGroupId::ROOT
        }
    }
}
