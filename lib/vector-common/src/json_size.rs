use std::{
    fmt,
    iter::Sum,
    num::NonZeroUsize,
    ops::{Add, AddAssign, Sub},
};

/// A newtype for the JSON size of an event.
/// Used to emit the `component_received_event_bytes_total` and
/// `component_sent_event_bytes_total` metrics.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct JsonSize(usize);

impl fmt::Display for JsonSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Sub for JsonSize {
    type Output = JsonSize;

    #[inline]
    fn sub(mut self, rhs: Self) -> Self::Output {
        self.0 -= rhs.0;
        self
    }
}

impl Add for JsonSize {
    type Output = JsonSize;

    #[inline]
    fn add(mut self, rhs: Self) -> Self::Output {
        self.0 += rhs.0;
        self
    }
}

impl AddAssign for JsonSize {
    #[inline]
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
    }
}

impl Sum for JsonSize {
    #[inline]
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        let mut accum = 0;
        for val in iter {
            accum += val.get();
        }

        JsonSize::new(accum)
    }
}

impl From<usize> for JsonSize {
    #[inline]
    fn from(value: usize) -> Self {
        Self(value)
    }
}

impl JsonSize {
    /// Create a new instance with the specified size.
    #[must_use]
    #[inline]
    pub const fn new(size: usize) -> Self {
        Self(size)
    }

    /// Create a new instance with size 0.
    #[must_use]
    #[inline]
    pub const fn zero() -> Self {
        Self(0)
    }

    /// Returns the contained size.
    #[must_use]
    #[inline]
    pub fn get(&self) -> usize {
        self.0
    }
}

/// Wraps `NonZeroUsize` so `Option<NonZeroJsonSize>` stays pointer-sized, which keeps
/// `AtomicCell<Option<NonZeroJsonSize>>` lock-free.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[allow(clippy::module_name_repetitions)]
pub struct NonZeroJsonSize(NonZeroUsize);

impl NonZeroJsonSize {
    #[must_use]
    #[inline]
    pub fn new(size: JsonSize) -> Option<Self> {
        NonZeroUsize::new(size.0).map(Self)
    }
}

impl From<NonZeroJsonSize> for JsonSize {
    #[inline]
    fn from(value: NonZeroJsonSize) -> Self {
        Self(value.0.get())
    }
}
