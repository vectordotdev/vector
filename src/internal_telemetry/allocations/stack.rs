use super::AllocationGroup;

/// An allocation group stack.
///
/// As allocation groups are entered and exited, they naturally end up looking a lot like a stack
/// itself: the active allocation group gets added to the stack when entered, and if another
/// allocation group is entered before the previous is exited, the newer group is added to the stack
/// above the previous one, and so on and so forth.
///
/// This implementation is uses an array to represent the stack to avoid thread local destructor
/// registration issues.
#[derive(Copy, Clone)]
pub struct GroupStack<const N: usize> {
    idx: usize,
    slots: [Option<&'static AllocationGroup>; N],
}

impl<const N: usize> GroupStack<N> {
    /// Creates an empty [`GroupStack`].
    pub const fn new() -> Self {
        Self {
            idx: 0,
            slots: [None; N],
        }
    }

    /// Gets the currently active allocation group.
    ///
    /// If the stack is empty, then the root allocation group is the defacto active allocation
    /// group, and is returned as such.
    pub const fn current(&self) -> Option<&'static AllocationGroup> {
        self.slots[self.idx]
    }

    /// Pushes an allocation group on to the stack, marking it as the active allocation group.
    pub fn push(&mut self, group: &'static AllocationGroup) {
        self.idx += 1;

        if self.idx >= self.slots.len() {
            panic!("tried to push new allocation group to the current stack, but hit the limit of {} entries", N);
        }

        self.slots[self.idx] = Some(group);
    }

    /// Pops the previous allocation group that was on the stack.
    ///
    /// Returns `Some(group)` if there's is a new active allocation group after popping, and `None`
    /// if the stack is now empty.
    pub fn pop(&mut self) -> Option<&'static AllocationGroup> {
        if self.idx == 0 {
            panic!("tried to pop current allocation group from the stack but the stack is empty");
        }

        // Clear the current slot.
        self.slots[self.idx].take();

        self.idx -= 1;
        self.slots[self.idx]
    }
}
