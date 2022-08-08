use crate::kind::Collection;

/// An `index` type that can be used in `Collection<Index>`
#[derive(Debug, Clone, Default, Copy, Eq, PartialEq, PartialOrd, Ord, Hash)]
pub struct Index(usize);

impl std::fmt::Display for Index {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl Index {
    /// Get the [`usize`] value of the index.
    #[must_use]
    pub const fn to_usize(self) -> usize {
        self.0
    }
}

impl Collection<Index> {
    /// Returns the largest known index, or None if no known indices exist.
    #[must_use]
    pub fn largest_known_index(&self) -> Option<usize> {
        self.known().keys().map(|i| i.to_usize()).max()
    }

    /// The minimum possible length an array could be given the type information.
    #[must_use]
    pub fn min_length(&self) -> usize {
        self.largest_known_index().map_or(0, |i| i + 1)
    }

    /// The exact length of the array, if it can be proven. Otherwise, None.
    #[must_use]
    pub fn exact_length(&self) -> Option<usize> {
        if self.unknown_kind().contains_any_defined() {
            None
        } else {
            // there are no defined unknown values, so all indices must be known
            Some(self.min_length())
        }
    }
}

impl From<usize> for Index {
    fn from(index: usize) -> Self {
        (&index).into()
    }
}

impl From<&usize> for Index {
    fn from(index: &usize) -> Self {
        Self(*index)
    }
}

impl From<Index> for usize {
    fn from(index: Index) -> Self {
        (&index).into()
    }
}

impl From<&Index> for usize {
    fn from(index: &Index) -> Self {
        index.0
    }
}

impl<T: Into<Self>> std::ops::Add<T> for Index {
    type Output = Self;

    fn add(self, other: T) -> Self {
        Self(self.0 + other.into().0)
    }
}

impl<T: Into<Self>> std::ops::Sub<T> for Index {
    type Output = Self;

    fn sub(self, other: T) -> Self {
        Self(self.0 - other.into().0)
    }
}
