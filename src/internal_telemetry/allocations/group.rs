use std::{
    cell::RefCell,
    num::NonZerousize,
    sync::atomic::{Atomicusize, Ordering},
};

use tracing::Span;

use super::tracing::WithAllocationGroup;

thread_local! {
    /// A stack representing the currently active allocation groups.
    ///
    /// As an allocation group is entered and exited, it will be pushed and popped from the group
    /// stack. Any allocations which occur on this thread will be associated with whichever
    /// allocation group is currently at the top of the stack at the time that the allocation
    /// occurs.
    static ALLOCATION_GROUP_STACK: RefCell<GroupStack<256>> =
        const { RefCell::new(GroupStack::new()) };
}

/// A registry of all registered allocation groups.
static GROUP_REGISTRY: Mutex<Vec<&'static AllocationGroup>> = Mutex::new(Vec::new());

/// An allocation group.
#[derive(Default)]
pub struct AllocationGroup {
    tags: Vec<(String, String)>,
    allocated_bytes: AtomicU64,
    allocations: AtomicU64,
    deallocated_bytes: AtomicU64,
    deallocations: AtomicU64,
}

impl AllocationGroup {
    /// Registers an allocation group with the given tags.
    ///
    /// An `AllocationGroupToken` is returned, which provides access to the underlying group as well
    /// as helper methods for dealing with associating (de)allocation events with the group.
    pub fn register(tags: Vec<(String, String)>) -> AllocationGroupToken {
        let allocation_group = Box::leak(Box::new(Self {
            tags,
            ..Default::default()
        }));

        // Register the allocation group in the group registry.
        let mut registry = GROUP_REGISTRY.lock().expect("antidote");
        registry.push(allocation_group);

        AllocationGroupToken(allocation_group)
    }

    pub fn track_allocation(&self, allocated_bytes: u64) {
        self.allocations.fetch_add(1, Ordering::Relaxed);
        self.allocated_bytes
            .fetch_add(allocated_bytes, Ordering::Relaxed);
    }

    pub fn track_deallocation(&self, deallocated_bytes: u64) {
        self.deallocations.fetch_add(1, Ordering::Relaxed);
        self.deallocated_bytes
            .fetch_add(deallocated_bytes, Ordering::Relaxed);
    }
}

/// A token tied to a specific allocation group.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct AllocationGroupToken(&'static AllocationGroup);

impl AllocationGroupToken {
    /// Gets a reference to the underlying allocation group.
    pub const fn group(&self) -> &'static AllocationGroup {
        self.0
    }

    pub fn enter(&self) {
        let _ = ALLOCATION_GROUP_STACK.try_with(|stack| stack.borrow_mut().push(self.0));
    }

    pub fn exit(&self) {
        let _ = ALLOCATION_GROUP_STACK.try_with(|stack| stack.borrow_mut().pop());
    }

    /// Attaches this allocation group to a [`Span`][tracing::Span].
    ///
    /// When the span is entered or exited, the allocation group will also transition from inactive
    /// to active, and vise versa. In effect, all allocations that occur while the span is entered
    /// will be associated with the allocation group.
    pub fn attach_to_span(self, span: &Span) {
        tracing::dispatcher::get_default(move |dispatch| {
            if let Some(id) = span.id() {
                if let Some(ctx) = dispatch.downcast_ref::<WithAllocationGroup>() {
                    (ctx.with_allocation_group)(dispatch, &id, self);
                }
            }
        });
    }
}

impl PartialEq for AllocationGroupToken {
    fn eq(&self, other: &Self) -> bool {
        self.0 as *const _ == other.0 as *const _
    }
}

impl Eq for AllocationGroupToken {}

impl Hash for AllocationGroupToken {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (self.0 as *const _).hash(state)
    }
}

/// Gets the current allocation group.
#[inline(always)]
pub(super) fn get_current_allocation_group() -> &'static AllocationGroup {
    ALLOCATION_GROUP_STACK.try_with(|stack| stack.borrow_mut().current());
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
    let _result = ALLOCATION_GROUP_STACK.try_with(
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
    let _result = ALLOCATION_GROUP_STACK.try_with(
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
