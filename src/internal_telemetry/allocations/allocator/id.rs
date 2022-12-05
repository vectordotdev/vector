use std::{
    cell::RefCell,
    num::NonZeroU8,
    sync::atomic::{AtomicU8, Ordering},
};

use tracing::Span;

use crate::internal_telemetry::allocations::allocator::storage::register_group_storage;

use super::{tracing::WithAllocationGroup, storage::AllocationGroupStorage};

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
            let group_id = AllocationGroupId::from_raw(group_id);
            register_group_storage(group_id);
            Some(group_id)
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
                    (ctx.with_allocation_group)(dispatch, &id, AllocationGroupStorage::from(self));
                }
            }
        });
    }
}

