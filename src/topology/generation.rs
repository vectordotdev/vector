use std::sync::atomic::{AtomicUsize, Ordering};

/// Generation counter incremented for each reload.
static RELOAD_GENERATION: AtomicUsize = AtomicUsize::new(0);

/// Increments generation causing all Ages to report that they
/// are old.
pub(super) fn inc_generation() {
    RELOAD_GENERATION.fetch_add(1, Ordering::Relaxed);
}

/// Age to be paired with some relodable data.
/// Age::new() -> is_old() -> set_age()
#[derive(Debug, PartialEq, Eq)]
pub struct Age {
    gen: usize,
}

impl Age {
    /// Creates new Age with current generation.
    pub fn new() -> Self {
        Age {
            gen: RELOAD_GENERATION.load(Ordering::Relaxed),
        }
    }

    /// Returns new age to be seted after the data
    /// has been reloaded.
    pub fn is_old(&self) -> Option<Age> {
        let current = Self::new();
        if current != *self {
            Some(current)
        } else {
            None
        }
    }

    /// Sets age to new value and checks wheter this new
    /// age is old in which case it returns even newer age.
    pub fn set_age(&mut self, new: Age) -> Option<Age> {
        *self = new;
        self.is_old()
    }
}
