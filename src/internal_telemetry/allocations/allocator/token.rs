use std::{
    cell::RefCell,
    num::NonZeroU8,
    sync::atomic::{AtomicU8, Ordering},
};

use tracing::Span;

use super::stack::GroupStack;
use super::tracing::WithAllocationGroup;

thread_local! {
    /// The currently executing allocation token.
    ///
    /// Any allocations which occur on this thread will be associated with whichever token is
    /// present at the time of the allocation.
    pub(crate) static LOCAL_ALLOCATION_GROUP_STACK: RefCell<GroupStack<256>> =
        const { RefCell::new(GroupStack::new()) };
}

/// The identifier that uniquely identifiers an allocation group.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AllocationGroupId(NonZeroU8);

impl AllocationGroupId {
    /// The group ID used for allocations which are not made within a registered allocation group.
    // Group IDs start at 1. The value 0 is reserved for handling runtime allocation edge cases.
    pub const ROOT: Self = AllocationGroupId::from_raw(1);

    pub(super) const fn from_raw(raw_group_id: u8) -> Self {
        unsafe { Self(NonZeroU8::new_unchecked(raw_group_id)) }
    }

    /// Gets the integer representation of this group ID.
    #[must_use]
    pub const fn as_raw(self) -> u8 {
        self.0.get()
    }

    /// Registers an allocation group ID.
    ///
    /// This group ID uniquely identifies a given allocation group, and is the means by which to
    /// distinguish allocator events between various allocation groups.
    ///
    /// Group IDs must be attached to a [`Span`][tracing::Span] in order to become active,
    /// associating allocations and deallocations within an active span as being attached to the
    /// given allocation group.
    pub fn register() -> Option<AllocationGroupId> {
        static GROUP_ID: AtomicU8 = AtomicU8::new(AllocationGroupId::ROOT.0.get() + 1);

        let group_id = GROUP_ID.fetch_add(1, Ordering::Relaxed);

        if group_id != u8::MAX {
            Some(AllocationGroupId::from_raw(group_id))
        } else {
            None
        }
    }

    /// Attaches this allocation group to a [`Span`][tracing::Span].
    ///
    /// When the span is entered or exited, the allocation group will also transition from inactive to active, and vise
    /// versa. In effect, all allocations that occur while the span is entered will be associated with the allocation
    /// group.
    pub fn attach_to_span(self, span: &Span) {
        tracing::dispatcher::get_default(move |dispatch| {
            if let Some(id) = span.id() {
                if let Some(ctx) = dispatch.downcast_ref::<WithAllocationGroup>() {
                    (ctx.with_allocation_group)(dispatch, &id, AllocationGroupToken::from(self));
                }
            }
        });
    }
}

/// A token that allows controlling when an allocation group is active or inactive.
pub struct AllocationGroupToken {
    id: AllocationGroupId,
}

impl AllocationGroupToken {
    pub fn enter(&self) {
        _ = LOCAL_ALLOCATION_GROUP_STACK.try_with(|stack| stack.borrow_mut().push(self.id));
    }

    pub fn exit(&self) {
        _ = LOCAL_ALLOCATION_GROUP_STACK.try_with(|stack| stack.borrow_mut().pop());
    }
}

impl From<AllocationGroupId> for AllocationGroupToken {
    fn from(group_id: AllocationGroupId) -> Self {
        Self { id: group_id }
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
/// In contrast to `try_with_suspended_allocation_group`, this method will always call `f` after attempting to suspend
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
