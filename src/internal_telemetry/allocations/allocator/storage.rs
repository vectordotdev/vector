use std::{sync::{atomic::{AtomicU64, Ordering}, Mutex}, cell::RefCell};

use crate::internal_telemetry::allocations::allocator::stack::GroupStack;

use super::AllocationGroupId;

thread_local! {
    /// The currently executing allocation token.
    ///
    /// Any allocations which occur on this thread will be associated with whichever token is
    /// present at the time of the allocation.
    static LOCAL_ALLOCATION_GROUP_STACK: RefCell<GroupStack<256>> =
        const { RefCell::new(GroupStack::new()) };
}

/// A registry for tracking each thread's group memory statistics.
static GROUP_STORAGE: Mutex<Vec<&'static AllocationGroupStorage>> = Mutex::new(Vec::new());

pub fn register_group_storage(group_id: AllocationGroupId) {
    let mut storage_refs = GROUP_STORAGE.lock().unwrap();

    // If this is the first real allocation group having storage registered, we need to do something
    // first: fill in two placeholders storage objects.
    //
    // One of them is just to fill the first slot, so that our first real group storage object
    // begins at index 1. This matches the group ID of the root allocation group.
    //
    // The other one is the group storage _for_ the root allocation group.
    if storage_refs.is_empty() {
        storage_refs.push(Box::leak(Box::new(AllocationGroupStorage::default())));
        storage_refs.push(Box::leak(Box::new(AllocationGroupStorage::default())));
    }

    // Make sure the group ID matches the number of group storage elements i.e. group ID ==
    // GROUP_STORAGE.len(). This ensures that when we actually register a group storage slot, its
    // index will map to the group ID.
    if storage_refs.len() != usize::from(group_id.as_raw()) {
        panic!("Allocation group ID ({}) should equal number of registered group storage objects ({}) prior to registration.", group_id.as_raw(), storage_refs.len());
    }

    storage_refs.push(Box::leak(Box::new(AllocationGroupStorage::default())));
}

pub fn get_group_storage_reference(group_id: AllocationGroupId) -> &'static AllocationGroupStorage {
    let idx = group_id.as_raw();
    GROUP_STORAGE.lock().unwrap().get(idx)
        .expect("allocation group registration should always create group storage")
}

#[derive(Default)]
pub struct AllocationGroupStorage {
    allocated_bytes: AtomicU64,
    allocations: AtomicU64,
    deallocated_bytes: AtomicU64,
    deallocations: AtomicU64,
}

impl AllocationGroupStorage {
    pub fn track_allocation(&self, object_size: u64) {
        self.allocated_bytes.fetch_add(object_size, Ordering::Release);
        self.allocations.fetch_add(1, Ordering::Release);
    }

    pub fn track_deallocation(&self, object_size: u64) {
        self.deallocated_bytes.fetch_add(object_size, Ordering::Release);
        self.deallocations.fetch_add(1, Ordering::Release);
    }

    pub fn get_allocation_stats(&self) -> (u64, u64) {
        (self.allocations.load(Ordering::Acquire), self.allocated_bytes.load(Ordering::Acquire))
    }

    pub fn get_deallocation_stats(&self) -> (u64, u64) {
        (self.deallocations.load(Ordering::Acquire), self.deallocated_bytes.load(Ordering::Acquire))
    }
}

/// A handle to the metric storage for a given allocation group.
pub struct AllocationGroupStorageHandle {
    group_storage: &'static AllocationGroupStorage,
}

impl AllocationGroupStorageHandle {
    pub fn activate(&self) {
        let _ = LOCAL_ALLOCATION_GROUP_STACK.try_with(|stack| stack.borrow_mut().push(self.group_storage));
    }

    pub fn deactivate(&self) {
        let _ = LOCAL_ALLOCATION_GROUP_STACK.try_with(|stack| stack.borrow_mut().pop());
    }
}

impl From<AllocationGroupId> for AllocationGroupStorageHandle {
    fn from(group_id: AllocationGroupId) -> Self {
        let group_storage = get_group_storage_reference(group_id);
        Self { group_storage }
    }
}

/// Calls `f` after suspending the active allocation group, if it was not already suspended.
///
/// If the active allocation group is not currently suspended, then `f` is called, after suspending it, with a reference
/// to the suspended allocation group. If any other call to `try_with_suspended_allocation_group` happens while this
/// method call is on the stack, `f` in those calls with itself not be called.
#[inline(always)]
pub(super) fn try_with_suspended_allocation_group<F>(f: F)
where
    F: FnOnce(AllocationGroupId),
{
    let _result = LOCAL_ALLOCATION_GROUP_STACK.try_with(
        #[inline(always)]
        |group_stack| {
            // The crux of avoiding reentrancy is `RefCell:try_borrow_mut`, which allows callers to skip trying to run
            // `f` if they cannot mutably borrow the current allocation group. As `try_borrow_mut` will only let one
            // mutable borrow happen at a time, the tracker logic is never reentrant.
            if let Ok(stack) = group_stack.try_borrow_mut() {
                f(stack.current());
            }
        },
    );
}

/// Calls `f` after suspending the active allocation group.
///
/// In constrast to `try_with_suspended_allocation_group`, this method will always call `f` after attempting to suspend
/// the active allocation group, even if it was already suspended.
///
/// In practice, this method is primarily useful for "run this function and don't trace any (de)allocations at all" while
/// `try_with_suspended_allocation_group` is primarily useful for "run this function if nobody else is tracing
/// an (de)allocation right now".
#[inline(always)]
pub(super) fn with_suspended_allocation_group<F>(f: F)
where
    F: FnOnce(),
{
    let _result = LOCAL_ALLOCATION_GROUP_STACK.try_with(
        #[inline(always)]
        |group_stack| {
            // The crux of avoiding reentrancy is `RefCell:try_borrow_mut`, as `try_borrow_mut` will only let one
            // mutable borrow happen at a time. As we simply want to ensure that the allocation group is suspended, we
            // don't care what the return value is: calling `try_borrow_mut` and holding on to the result until the end
            // of the scope is sufficient to either suspend the allocation group or know that it's already suspended and
            // will stay that way until we're done in this method.
            let _result = group_stack.try_borrow_mut();
            f();
        },
    );
}
