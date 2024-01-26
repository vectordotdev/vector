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
    pub const fn new() -> Self {
        Self {
            current_top: 0,
            slots: [AllocationGroupId::ROOT; N],
        }
    }

    /// Gets the currently active allocation group.
    ///
    /// If the stack is empty, then the root allocation group is the defacto active allocation group, and is returned as such.
    pub const fn current(&self) -> AllocationGroupId {
        self.slots[self.current_top]
    }

    /// Pushes an allocation group on to the stack, marking it as the active allocation group.
    pub fn push(&mut self, group: AllocationGroupId) {
        self.current_top += 1;
        if self.current_top >= self.slots.len() {
            panic!("tried to push new allocation group to the current stack, but hit the limit of {} entries", N);
        }
        self.slots[self.current_top] = group;
    }

    /// Pops the previous allocation group that was on the stack.
    pub fn pop(&mut self) {
        if self.current_top == 0 {
            panic!("tried to pop current allocation group from the stack but the stack is empty");
        }
        self.current_top -= 1;
    }
}
