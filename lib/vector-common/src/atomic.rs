use std::sync::atomic::{AtomicU64, Ordering};

use metrics::GaugeFn;

/// Simple atomic wrapper for `f64` values.
#[derive(Debug)]
pub struct AtomicF64(AtomicU64);

impl AtomicF64 {
    /// Creates a new `AtomicF64` with the given initial value.
    #[must_use]
    pub fn new(init: f64) -> Self {
        Self(AtomicU64::new(init.to_bits()))
    }

    pub fn load(&self, order: Ordering) -> f64 {
        f64::from_bits(self.0.load(order))
    }

    #[expect(clippy::missing_panics_doc, reason = "fetch_update always succeeds")]
    pub fn fetch_update(
        &self,
        set_order: Ordering,
        fetch_order: Ordering,
        mut f: impl FnMut(f64) -> f64,
    ) -> f64 {
        f64::from_bits(
            self.0
                .fetch_update(set_order, fetch_order, |x| {
                    Some(f(f64::from_bits(x)).to_bits())
                })
                .expect("fetch_update always succeeds"),
        )
    }
}

impl GaugeFn for AtomicF64 {
    fn increment(&self, amount: f64) {
        self.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| value + amount);
    }

    fn decrement(&self, amount: f64) {
        self.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| value - amount);
    }

    fn set(&self, value: f64) {
        self.0.store(f64::to_bits(value), Ordering::Relaxed);
    }
}
