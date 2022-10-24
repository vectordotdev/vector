use crate::kind::collection::CollectionRemove;
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
        self.known()
            .iter()
            .filter_map(|(i, kind)| {
                if kind.contains_any_defined() {
                    Some(i.to_usize())
                } else {
                    None
                }
            })
            .max()
    }

    /// Converts a negative index to a positive index (only if the exact positive index is known).
    #[must_use]
    pub fn get_positive_index(&self, index: isize) -> Option<usize> {
        if self.unknown_kind().contains_any_defined() {
            // positive index can't be known if there are unknown values
            return None;
        }

        let negative_index = (-index) as usize;
        if let Some(largest_known_index) = self.largest_known_index() {
            if largest_known_index >= negative_index - 1 {
                // The exact index to remove is known.
                return Some(((largest_known_index as isize) + 1 + index) as usize);
            }
        }
        // Removing a non-existing index
        None
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

    /// Removes the known value at the given index and shifts the
    /// elements to the left.
    pub fn remove_shift(&mut self, index: usize) {
        let min_length = self.min_length();
        self.known_mut().remove(&index.into());
        for i in index..min_length {
            if let Some(value) = self.known_mut().remove(&(index + 1).into()) {
                self.known_mut().insert(index.into(), value);
            }
        }
    }
}

impl CollectionRemove for Collection<Index> {
    type Key = Index;

    fn remove_known(&mut self, key: &Index) {
        self.remove_shift(key.0);
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
