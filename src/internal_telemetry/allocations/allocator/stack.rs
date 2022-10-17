use super::token::AllocationGroupId;

/// An allocation group stack.
///
/// As allocation groups are entered and exited, they naturally end up looking a lot like a stack itself: the active
/// allocation group gets added to the stack when entered, and if another allocation group is entered before the
/// previous is exited, the newer group is added to the stack above the previous one, and so on and so forth.
///
/// This implementation is an incredibly thin wrapper around `Vec<T>` which already provides the necessary "push" and
/// "pop" methods required for a stack. Our logic is slightly tweaked to account for the expectation that a there should
/// never be a pop without a corresponding push, and so on.
#[derive(Copy, Clone)]
pub(crate) struct GroupStack {
    slots: [AllocationGroupId; 512],
    current_val: usize,
}

impl GroupStack {
    /// Creates an empty [`GroupStack`].
    pub const fn new() -> Self {
        Self {
            current_val: 0,
            slots: [AllocationGroupId::from_raw_unchecked(1); 512],
        }
    }

    /// Gets the currently active allocation group.
    ///
    /// If the stack is empty, then the root allocation group is the defacto active allocation group, and is returned as such.
    pub const fn current(&self) -> AllocationGroupId {
        if self.current_val == 0 {
            AllocationGroupId::ROOT
        } else {
            self.slots[self.current_val - 1]
        }
    }

    /// Pushes an allocation group on to the stack, marking it as the active allocation group.
    pub fn push(&mut self, group: AllocationGroupId) {
        self.slots[self.current_val] = group;
        self.current_val += 1;
    }

    /// Pops the currently active allocation group off the stack.
    pub fn pop(&mut self) -> AllocationGroupId {
        self.current_val -= 1;
        self.slots[self.current_val]
    }
}
