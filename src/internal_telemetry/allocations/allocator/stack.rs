use super::token::AllocationGroupId;

/// An allocation group stack.
///
/// As allocation groups are entered and exited, they naturally end up looking a lot like a stack itself: the active
/// allocation group gets added to the stack when entered, and if another allocation group is entered before the
/// previous is exited, the newer group is added to the stack above the previous one, and so on and so forth.
///
/// This implementation is uses an array to represent the stack to avoid thread local destructor registration issues.
#[derive(Copy, Clone)]
pub(crate) struct GroupStack {
    slots: [AllocationGroupId; 256],
    current_top: usize,
}

impl GroupStack {
    /// Creates an empty [`GroupStack`].
    pub const fn new() -> Self {
        Self {
            current_top: 0,
            slots: [AllocationGroupId::ROOT; 256],
        }
    }

    /// Gets the currently active allocation group.
    ///
    /// If the stack is empty, then the root allocation group is the defacto active allocation group, and is returned as such.
    pub const fn current(&self) -> AllocationGroupId {
        if self.current_top == 0 {
            AllocationGroupId::ROOT
        } else {
            self.slots[self.current_top - 1]
        }
    }

    /// Pushes an allocation group on to the stack, marking it as the active allocation group.
    pub fn push(&mut self, group: AllocationGroupId) {
        if self.current_top >= self.slots.len() {
            panic!("stack overflow");
        }
        self.slots[self.current_top] = group;
        self.current_top += 1;
    }

    /// Pops the currently active allocation group off the stack.
    pub fn pop(&mut self) -> AllocationGroupId {
        if self.current_top == 0 {
            panic!("stack underflow!");
        }
        self.current_top -= 1;
        self.slots[self.current_top]
    }
}
