use std::{
    cell::RefCell,
    num::NonZeroUsize,
    sync::atomic::{AtomicUsize, Ordering},
};

use tracing::Span;

use super::tracing::WithAllocationGroup;

thread_local! {
    /// The currently executing allocation group.
    ///
    /// Any allocations which occur on this thread will be associated with whichever token is
    /// present at the time of the allocation.
    pub static CURRENT_ALLOCATION_GROUP: RefCell<AllocationGroupId> = RefCell::new(AllocationGroupId::ROOT);
}

/// The identifier that uniquely identifies an allocation group.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AllocationGroupId(NonZeroUsize);

impl AllocationGroupId {
    /// The group ID used for allocations which are not made within a registered allocation group.
    pub const ROOT: Self = Self(unsafe { NonZeroUsize::new_unchecked(1) });

    pub(super) const fn from_raw(raw_group_id: usize) -> Option<Self> {
        match NonZeroUsize::new(raw_group_id) {
            Some(id) => Some(Self(id)),
            None => None,
        }
    }

    /// Gets the integer representation of this group ID.
    #[must_use]
    pub const fn as_raw(self) -> usize {
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
        static GROUP_ID: AtomicUsize = AtomicUsize::new(AllocationGroupId::ROOT.0.get() + 1);
        static HIGHEST_GROUP_ID: AtomicUsize =
            AtomicUsize::new(AllocationGroupId::ROOT.0.get() + 1);

        let group_id = GROUP_ID.fetch_add(1, Ordering::Relaxed);
        let highest_group_id = HIGHEST_GROUP_ID.fetch_max(group_id, Ordering::AcqRel);

        if group_id >= highest_group_id {
            let group_id = NonZeroUsize::new(group_id).expect("bug: GROUP_ID overflowed");
            Some(AllocationGroupId(group_id))
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
                    ctx.with_allocation_group(dispatch, &id, AllocationGroupToken::from(self));
                }
            }
        });
    }
}

/// A token that allows controlling when an allocation group is active or inactive.
pub struct AllocationGroupToken {
    previous: Option<AllocationGroupId>,
    id: AllocationGroupId,
}

impl AllocationGroupToken {
    pub fn enter(&mut self) {
        if self.previous.is_some() {
            panic!(
                "Should not be entering a token which has already been entered: previous={}, id={}",
                self.previous.unwrap().as_raw(),
                self.id.as_raw()
            );
        }

        let previous = set_current_allocation_group(self.id);
        self.previous = Some(previous);
    }

    pub fn exit(&mut self) {
        match self.previous.take() {
            None => panic!(
                "Should not be exiting a token which has not yet been entered: id={}",
                self.id.as_raw()
            ),
            Some(previous) => {
                let current = set_current_allocation_group(previous);
                if self.id != current {
                    panic!(
                        "Expected current allocation group for thread to be {}, but got {}.",
                        self.id.as_raw(),
                        current.as_raw()
                    );
                }
            }
        }
    }
}

impl From<AllocationGroupId> for AllocationGroupToken {
    fn from(group_id: AllocationGroupId) -> Self {
        Self {
            previous: None,
            id: group_id,
        }
    }
}

fn set_current_allocation_group(group_id: AllocationGroupId) -> AllocationGroupId {
    // If the current thread is being deinitialized, we can't actually access the value, so we just
    // return a default of `AllocationGroupId::ROOT`.
    CURRENT_ALLOCATION_GROUP
        .try_with(|current_group_id| current_group_id.replace(group_id))
        .unwrap_or(AllocationGroupId::ROOT)
}

/// Calls `f` after suspending the active allocation group, if it was not already suspended.
///
/// If the active allocation group is not currently suspended, then `f` is called, after suspending it, with a reference
/// to the suspended allocation group. If any other call to `try_with_suspended_allocation_group` happens while this
/// method call is on the stack, `f` in those calls with itself not be called.
pub(super) fn try_with_suspended_allocation_group<F>(f: F)
where
    F: FnOnce(AllocationGroupId),
{
    let _result = CURRENT_ALLOCATION_GROUP.try_with(|current_group_id| {
        // The crux of avoiding reentrancy is `RefCell:try_borrow_mut`, which allows callers to skip trying to run
        // `f` if they cannot mutably borrow the current allocation group. As `try_borrow_mut` will only let one
        // mutable borrow happen at a time, the tracker logic is never reentrant.
        if let Ok(group_id) = current_group_id.try_borrow_mut() {
            f(*group_id);
        }
    });
}

/// Calls `f` after suspending the active allocation group.
///
/// In contrast to `try_with_suspended_allocation_group`, this method will always call `f` after attempting to suspend
/// the active allocation group, even if it was already suspended.
///
/// In practice, this method is primarily useful for "run this function and don't trace any (de)allocations at all" while
/// `try_with_suspended_allocation_group` is primarily useful for "run this function if nobody else is tracing
/// an (de)allocation right now".
pub(super) fn with_suspended_allocation_group<F>(f: F)
where
    F: FnOnce(),
{
    let _result = CURRENT_ALLOCATION_GROUP.try_with(|current_group_id| {
        // The crux of avoiding reentrancy is `RefCell:try_borrow_mut`, as `try_borrow_mut` will only let one
        // mutable borrow happen at a time. As we simply want to ensure that the allocation group is suspended, we
        // don't care what the return value is: calling `try_borrow_mut` and holding on to the result until the end
        // of the scope is sufficient to either suspend the allocation group or know that it's already suspended and
        // will stay that way until we're done in this method.
        let _result = current_group_id.try_borrow_mut();
        f();
    });
}
