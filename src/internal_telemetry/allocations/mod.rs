//! Allocation tracking exposed via internal telemetry.

mod allocator;
use std::{
    cell::Cell,
    sync::{
        atomic::{AtomicI64, AtomicUsize, Ordering},
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

use crossbeam_utils::CachePadded;

const NUM_GROUPS: usize = 128;
const NUM_BUCKETS: usize = 8;
/// These arrays represent the memory usage for each group per thread.
///
/// Each thread is meant to update it's own group statistics, which significantly reduces atomic contention.
/// We pad each Atomic to reduce false sharing effects.
///
/// TODO:
///
/// Currently, we reach atomic contention once the number of threads exceeds 8. One potential solution
/// involves using thread locals to track memory stats.
static GROUP_MEM_STATS: [[CachePadded<AtomicI64>; NUM_GROUPS]; NUM_BUCKETS] = [
    arr![CachePadded::new(AtomicI64::new(0)); 128],
    arr![CachePadded::new(AtomicI64::new(0)); 128],
    arr![CachePadded::new(AtomicI64::new(0)); 128],
    arr![CachePadded::new(AtomicI64::new(0)); 128],
    arr![CachePadded::new(AtomicI64::new(0)); 128],
    arr![CachePadded::new(AtomicI64::new(0)); 128],
    arr![CachePadded::new(AtomicI64::new(0)); 128],
    arr![CachePadded::new(AtomicI64::new(0)); 128],
];

// TODO: Replace this with [`std::thread::ThreadId::as_u64`] once it is stabilized.
static THREAD_COUNTER: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    static THREAD_ID: Cell<usize> = const { Cell::new(0) };
}
// By using the Option type, we can do statics w/o the need of other creates such as lazy_static
struct GroupInfo {
    component_kind: Option<String>,
    component_type: Option<String>,
    component_id: Option<String>,
}

impl GroupInfo {
    const fn new() -> Self {
        Self {
            component_id: None,
            component_kind: None,
            component_type: None,
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
        GROUP_MEM_STATS[THREAD_ID.with(|t| t.get()) % 8][group_id.as_raw()]
            .fetch_add(object_size as i64, Ordering::Relaxed);
    }

    #[inline(always)]
    fn trace_deallocation(&self, object_size: usize, source_group_id: AllocationGroupId) {
        GROUP_MEM_STATS[THREAD_ID.with(|t| t.get()) % 8][source_group_id.as_raw()]
            .fetch_sub(object_size as i64, Ordering::Relaxed);
    }
}

/// Initializes allocation tracing.
pub fn init_allocation_tracing() {
    let alloc_processor = thread::Builder::new().name("vector-alloc-processor".to_string());
    alloc_processor
        .spawn(|| {
            without_allocation_tracing(|| loop {
                for group_idx in 0..NUM_GROUPS {
                    let mut mem_used = 0;
                    for bucket in &GROUP_MEM_STATS {
                        mem_used += bucket[group_idx].load(Ordering::Relaxed);
                    }
                    if mem_used == 0 {
                        continue;
                    }
                    let group_info = GROUP_INFO[group_idx].lock().unwrap();
                    gauge!(
                        "component_allocated_bytes",
                        mem_used.to_f64().expect("failed to convert group_id from int to float"),
                        "component_kind" => group_info.component_kind.clone().unwrap_or_else(|| "root".to_string()),
                        "component_type" => group_info.component_type.clone().unwrap_or_else(|| "root".to_string()),
                        "component_id" => group_info.component_id.clone().unwrap_or_else(|| "root".to_string()));
                }
                thread::sleep(Duration::from_millis(5000));
            })
        })
        .unwrap();
}

/// Initializes the thread local ID.
pub fn init_thread_id() {
    THREAD_ID.with(|t| t.replace(THREAD_COUNTER.fetch_add(1, Ordering::Relaxed)));
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
    match GROUP_INFO.get(idx) {
        Some(mutex) => {
            let mut writer = mutex.lock().unwrap();
            *writer = GroupInfo {
                component_id: Some(component_id),
                component_kind: Some(component_kind),
                component_type: Some(component_type),
            };
            group_id
        }
        None => {
            info!("Maximum number of registrable allocation group IDs reached ({}). Allocations for component '{}' will be attributed to the root allocation group.", NUM_GROUPS, component_id);
            AllocationGroupId::ROOT
        }
    }
}
