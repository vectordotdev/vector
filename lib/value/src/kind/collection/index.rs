/// An `index` type that can be used in `Collection<Index>`
#[derive(Debug, Clone, Default, Copy, Eq, PartialEq, PartialOrd, Ord)]
pub struct Index(usize);

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

impl<T: Into<Index>> std::ops::Add<T> for Index {
    type Output = Self;

    fn add(self, other: T) -> Self {
        Self(self.0 + other.into().0)
    }
}

impl<T: Into<Index>> std::ops::Sub<T> for Index {
    type Output = Self;

    fn sub(self, other: T) -> Self {
        Self(self.0 - other.into().0)
    }
}
