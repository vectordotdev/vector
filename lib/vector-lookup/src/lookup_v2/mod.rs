mod optional_path;

pub use optional_path::{OptionalTargetPath, OptionalValuePath};
use vector_config_macros::configurable_component;

pub use lookup::lookup_v2::{
    parse_target_path, parse_value_path, BorrowedSegment, OwnedSegment, OwnedTargetPath,
    OwnedValuePath, PathConcat, PathParseError, PathPrefix, TargetPath, ValuePath,
};

/// A wrapper around `OwnedValuePath` that allows it to be used in Vector config
#[configurable_component]
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(try_from = "String", into = "String")]
pub struct ConfigOwnedValuePath(pub OwnedValuePath);

impl TryFrom<String> for ConfigOwnedValuePath {
    type Error = PathParseError;

    fn try_from(src: String) -> Result<Self, Self::Error> {
        OwnedValuePath::try_from(src).map(ConfigOwnedValuePath)
    }
}

impl From<ConfigOwnedValuePath> for String {
    fn from(owned: ConfigOwnedValuePath) -> Self {
        String::from(owned.0)
    }
}

impl<'a> ValuePath<'a> for &'a ConfigOwnedValuePath {
    type Iter = <&'a OwnedValuePath as ValuePath<'a>>::Iter;

    fn segment_iter(&self) -> Self::Iter {
        (&self.0).segment_iter()
    }
}

/// A wrapper around `OwnedTargetPath` that allows it to be used in Vector config
/// with prefix default to `PathPrefix::Event`
#[configurable_component]
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(try_from = "String", into = "String")]
pub struct ConfigOwnedTargetPath(pub OwnedTargetPath);

impl TryFrom<String> for ConfigOwnedTargetPath {
    type Error = PathParseError;

    fn try_from(src: String) -> Result<Self, Self::Error> {
        OwnedTargetPath::try_from(src).map(ConfigOwnedTargetPath)
    }
}

impl From<ConfigOwnedTargetPath> for String {
    fn from(owned: ConfigOwnedTargetPath) -> Self {
        String::from(owned.0)
    }
}

impl<'a> TargetPath<'a> for &'a ConfigOwnedTargetPath {
    type ValuePath = &'a OwnedValuePath;

    fn prefix(&self) -> PathPrefix {
        self.0.prefix
    }

    fn value_path(&self) -> Self::ValuePath {
        &self.0.path
    }
}
