/// The user can configure whether a tag should be emitted. If they configure it to
/// be emitted, but the value doesn't exist - we should emit the tag but with a value
/// of `-`.
#[derive(Clone, Debug, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub enum OptionalTag {
    Ignored,
    Specified(Option<String>),
}

impl From<Option<String>> for OptionalTag {
    fn from(value: Option<String>) -> Self {
        Self::Specified(value)
    }
}
