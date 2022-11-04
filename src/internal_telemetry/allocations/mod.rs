//! Allocation tracking exposed via internal telemetry.

mod allocator;
use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
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

const NUM_GROUPS: usize = 256;
/// These arrays represent the allocations and deallocations for each group.
/// We pad each Atomic to reduce false sharing effects.
static GROUP_MEM_ALLOCS: [CachePadded<AtomicU64>; NUM_GROUPS] =
    arr![CachePadded::new(AtomicU64::new(0)); 256];
static GROUP_MEM_DEALLOCS: [CachePadded<AtomicU64>; NUM_GROUPS] =
    arr![CachePadded::new(AtomicU64::new(0)); 256];

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

static GROUP_INFO: [Mutex<GroupInfo>; NUM_GROUPS] = arr![Mutex::new(GroupInfo::new()); 256];

pub type Allocator<A> = GroupedTraceableAllocator<A, MainTracer>;

pub const fn get_grouped_tracing_allocator<A>(allocator: A) -> Allocator<A> {
    GroupedTraceableAllocator::new(allocator, MainTracer)
}

pub struct MainTracer;

impl Tracer for MainTracer {
    #[inline(always)]
    fn trace_allocation(&self, object_size: usize, group_id: AllocationGroupId) {
        GROUP_MEM_ALLOCS[group_id.as_raw()].fetch_add(object_size as u64, Ordering::Relaxed);
    }

    #[inline(always)]
    fn trace_deallocation(&self, object_size: usize, source_group_id: AllocationGroupId) {
        GROUP_MEM_DEALLOCS[source_group_id.as_raw()]
            .fetch_add(object_size as u64, Ordering::Relaxed);
    }
}

/// Initializes allocation tracing.
pub fn init_allocation_tracing() {
    let alloc_processor = thread::Builder::new().name("vector-alloc-processor".to_string());
    alloc_processor
        .spawn(|| {
            without_allocation_tracing(|| loop {
                for idx in 0..GROUP_MEM_ALLOCS.len() {
                    let allocs = GROUP_MEM_ALLOCS[idx].load(Ordering::Relaxed);
                    let deallocs = GROUP_MEM_DEALLOCS[idx].load(Ordering::Relaxed);
                    let mem_used = allocs - deallocs;

                    if allocs == 0 {
                        continue;
                    }
                    let group_info = GROUP_INFO[idx].lock().unwrap();
                    gauge!(
                        "component_allocated_bytes",
                        mem_used.to_f64().expect("failed to convert group_id from int to float"),
                        "group_id" => idx.to_string(),
                        "component_kind" => group_info.component_kind.clone().unwrap_or_else(|| "root".to_string()),
                        "component_type" => group_info.component_type.clone().unwrap_or_else(|| "root".to_string()),
                        "component_id" => group_info.component_id.clone().unwrap_or_else(|| "root".to_string()));
                }
                thread::sleep(Duration::from_millis(5000));
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
            info!("We've reached the maximum number of allocation group IDs: {}. The allocations for the component {} will be attributed to the root allocation group.", NUM_GROUPS, component_id);
            AllocationGroupId::ROOT
        }
    }
}
