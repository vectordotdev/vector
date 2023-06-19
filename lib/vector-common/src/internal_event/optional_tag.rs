/// The user can configure whether a tag should be emitted. If they configure it to
/// be emitted, but the value doesn't exist - we should emit the tag but with a value
/// of `-`.
#[derive(Clone, Debug, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub enum OptionalTag<T> {
    Ignored,
    Specified(Option<T>),
}

impl<T> From<Option<T>> for OptionalTag<T> {
    fn from(value: Option<T>) -> Self {
        Self::Specified(value)
    }
}
