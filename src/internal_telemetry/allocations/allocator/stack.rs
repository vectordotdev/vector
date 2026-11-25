use super::token::AllocationGroupId;

/// An allocation group stack.
///
/// As allocation groups are entered and exited, they naturally end up looking a lot like a stack itself: the active
/// allocation group gets added to the stack when entered, and if another allocation group is entered before the
/// previous is exited, the newer group is added to the stack above the previous one, and so on and so forth.
///
/// This implementation is uses an array to represent the stack to avoid thread local destructor registration issues.
#[derive(Copy, Clone)]
pub(crate) struct GroupStack<const N: usize> {
    current_top: usize,
    slots: [AllocationGroupId; N],
}

impl<const N: usize> GroupStack<N> {
    /// Creates an empty [`GroupStack`].
    pub(super) const fn new() -> Self {
        Self {
            current_top: 0,
            slots: [AllocationGroupId::ROOT; N],
        }
    }

    /// Gets the currently active allocation group.
    ///
    /// If the stack is empty, then the root allocation group is the defacto active allocation group, and is returned as such.
    pub(super) const fn current(&self) -> AllocationGroupId {
        self.slots[self.current_top]
    }

    /// Pushes an allocation group on to the stack, marking it as the active allocation group.
    pub(super) fn push(&mut self, group: AllocationGroupId) {
        self.current_top += 1;
        assert!(
            self.current_top < self.slots.len(),
            "tried to push new allocation group to the current stack, but hit the limit of {N} entries"
        );
        self.slots[self.current_top] = group;
    }

    /// Pops the previous allocation group that was on the stack.
    pub(super) fn pop(&mut self) {
        assert!(
            self.current_top != 0,
            "tried to pop current allocation group from the stack but the stack is empty"
        );
        self.current_top -= 1;
    }
}

#[cfg(test)]
mod tests {
    use std::panic::{AssertUnwindSafe, catch_unwind};

    use super::*;

    #[test]
    fn stack_restores_groups_and_enforces_bounds() {
        let root = AllocationGroupId::ROOT;
        let first = AllocationGroupId::from_raw(2);
        let second = AllocationGroupId::from_raw(3);
        let mut stack = GroupStack::<3>::new();
        assert_eq!(stack.current(), root);
        stack.push(first);
        stack.push(second);
        assert_eq!(stack.current(), second);
        stack.pop();
        assert_eq!(stack.current(), first);
        stack.push(second);
        assert_eq!(stack.current(), second);
        stack.pop();
        stack.pop();
        assert_eq!(stack.current(), root);

        let underflow = catch_unwind(AssertUnwindSafe(|| stack.pop())).unwrap_err();
        assert_eq!(
            underflow.downcast_ref::<&str>().copied(),
            Some("tried to pop current allocation group from the stack but the stack is empty")
        );
        stack.push(first);
        stack.push(second);
        let overflow = catch_unwind(AssertUnwindSafe(|| stack.push(first))).unwrap_err();
        assert_eq!(
            overflow.downcast_ref::<String>().unwrap(),
            "tried to push new allocation group to the current stack, but hit the limit of 3 entries"
        );
    }
}
