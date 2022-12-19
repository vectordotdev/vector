use std::{
    borrow::Cow,
    cell::UnsafeCell,
    hash::Hasher,
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
};

use tracing::Span;

use super::tracing::WithAllocationGroup;
use super::{accumulator::Accumulator, stack::GroupStack};

thread_local! {
    /// A stack representing the currently active allocation groups.
    ///
    /// As an allocation group is entered and exited, it will be pushed and popped from the group
    /// stack. Any allocations which occur on this thread will be associated with whichever
    /// allocation group is currently at the top of the stack at the time that the allocation
    /// occurs.
    static ALLOCATION_GROUP_STACK: UnsafeCell<GroupStack<256>> = const { UnsafeCell::new(GroupStack::new()) };
}

/// A registry of all registered allocation groups.
static GROUP_REGISTRY: Mutex<Vec<&'static AllocationGroup>> = Mutex::new(Vec::new());

/// Root allocation group.
///
/// All (de)allocations that occur when no other allocation group is active will be associated with
/// the root allocation group.
static ROOT_ALLOCATION_GROUP: AllocationGroup = AllocationGroup::root();

/// An allocation group.
#[derive(Debug, Default)]
pub struct AllocationGroup {
    pub component_id: Cow<'static, str>,
    pub component_kind: Cow<'static, str>,
    pub component_type: Cow<'static, str>,
    allocated_bytes: AtomicU64,
    deallocated_bytes: AtomicU64,
}

impl AllocationGroup {
    const fn new(
        component_id: Cow<'static, str>,
        component_kind: Cow<'static, str>,
        component_type: Cow<'static, str>,
    ) -> Self {
        AllocationGroup {
            component_id,
            component_kind,
            component_type,
            allocated_bytes: AtomicU64::new(0),
            deallocated_bytes: AtomicU64::new(0),
        }
    }

    const fn root() -> Self {
        Self::new(
            Cow::Borrowed("root"),
            Cow::Borrowed("root"),
            Cow::Borrowed("root"),
        )
    }

    /// Registers an allocation group with the given tags.
    ///
    /// An `AllocationGroupToken` is returned, which provides access to the underlying group as well
    /// as helper methods for dealing with associating (de)allocation events with the group.
    pub fn register(
        component_id: &str,
        component_kind: &str,
        component_type: &str,
    ) -> AllocationGroupToken {
        let allocation_group = Box::leak(Box::new(Self::new(
            component_id.to_string().into(),
            component_kind.to_string().into(),
            component_type.to_string().into(),
        )));

        // Register the allocation group in the group registry.
        let mut registry = GROUP_REGISTRY.lock().expect("antidote");
        registry.push(allocation_group);

        AllocationGroupToken(allocation_group)
    }

    pub fn track_allocation(&self, allocated_bytes: u64) {
        self.allocated_bytes
            .fetch_add(allocated_bytes, Ordering::Relaxed);
    }

    pub fn track_deallocation(&self, deallocated_bytes: u64) {
        self.deallocated_bytes
            .fetch_add(deallocated_bytes, Ordering::Relaxed);
    }

    pub fn consume_and_reset_statistics(&self) -> (u64, u64) {
        let allocated_bytes = self.allocated_bytes.swap(0, Ordering::Relaxed);
        let deallocated_bytes = self.deallocated_bytes.swap(0, Ordering::Relaxed);

        (allocated_bytes, deallocated_bytes)
    }
}

/// A token tied to a specific allocation group.
#[derive(Clone, Copy, Debug)]
pub struct AllocationGroupToken(&'static AllocationGroup);

impl AllocationGroupToken {
    /// Gets a reference to the underlying allocation group.
    pub const fn group(&self) -> &'static AllocationGroup {
        self.0
    }

    pub fn enter(&self) {
        // SAFETY: The group stack is per-thread, so we are the only ones that could possibly be
        // accessing it here.
        unsafe {
            let _ = ALLOCATION_GROUP_STACK.try_with(|stack| {
                (&mut *stack.get()).push(self.0);
                Accumulator::enter(self.0);
            });
        }
    }

    pub fn exit(&self) {
        // SAFETY: The group stack is per-thread, so we are the only ones that could possibly be
        // accessing it here.
        unsafe {
            let _ = ALLOCATION_GROUP_STACK.try_with(|stack| {
                if let Some(new_group) = (&mut *stack.get()).pop() {
                    Accumulator::enter(new_group);
                } else {
                    Accumulator::exit();
                }
            });
        }
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

impl std::hash::Hash for AllocationGroupToken {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (self.0 as *const AllocationGroup).hash(state)
    }
}

pub fn get_registered_allocation_groups() -> Vec<&'static AllocationGroup> {
    let registry = GROUP_REGISTRY.lock().expect("antidote");
    registry.clone()
}

/// Gets the current allocation group.
#[inline(always)]
pub(super) fn get_current_allocation_group() -> &'static AllocationGroup {
    // SAFETY: The group stack is per-thread, so we are the only ones that could possibly be
    // accessing it here.
    unsafe {
        ALLOCATION_GROUP_STACK
            .try_with(|stack| (&*stack.get()).current())
            .unwrap_or_else(|_| Some(&ROOT_ALLOCATION_GROUP))
            .unwrap_or_else(|| &ROOT_ALLOCATION_GROUP)
    }
}
