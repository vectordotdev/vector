mod optional_path;

pub use optional_path::{OptionalTargetPath, OptionalValuePath};
use vector_config_macros::configurable_component;

pub use vrl::path::{
    parse_target_path, parse_value_path, BorrowedSegment, OwnedSegment, OwnedTargetPath,
    OwnedValuePath, PathConcat, PathParseError, PathPrefix, TargetPath, ValuePath,
};

/// A wrapper around `OwnedValuePath` that allows it to be used in Vector config.
/// This requires a valid path to be used. If you want to allow optional paths,
/// use [optional_path::OptionalValuePath].
#[configurable_component]
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(try_from = "String", into = "String")]
pub struct ConfigValuePath(pub OwnedValuePath);

impl TryFrom<String> for ConfigValuePath {
    type Error = PathParseError;

    fn try_from(src: String) -> Result<Self, Self::Error> {
        OwnedValuePath::try_from(src).map(ConfigValuePath)
    }
}

impl From<ConfigValuePath> for String {
    fn from(owned: ConfigValuePath) -> Self {
        String::from(owned.0)
    }
}

impl<'a> ValuePath<'a> for &'a ConfigValuePath {
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
pub struct ConfigTargetPath(pub OwnedTargetPath);

impl TryFrom<String> for ConfigTargetPath {
    type Error = PathParseError;

    fn try_from(src: String) -> Result<Self, Self::Error> {
        OwnedTargetPath::try_from(src).map(ConfigTargetPath)
    }
}

impl From<ConfigTargetPath> for String {
    fn from(owned: ConfigTargetPath) -> Self {
        String::from(owned.0)
    }
}

impl<'a> TargetPath<'a> for &'a ConfigTargetPath {
    type ValuePath = &'a OwnedValuePath;

    fn prefix(&self) -> PathPrefix {
        self.0.prefix
    }

    fn value_path(&self) -> Self::ValuePath {
        &self.0.path
    }
}
