use vector_config::configurable_component;

use crate::lookup_v2::PathParseError;
use crate::{OwnedTargetPath, OwnedValuePath};

#[configurable_component]
#[derive(Debug, Clone, PartialEq, Eq, Default, Hash, PartialOrd, Ord)]
#[serde(try_from = "String", into = "String")]
/// An optional path that deserializes an empty string to `None`.
pub struct OptionalTargetPath {
    pub path: Option<OwnedTargetPath>,
}

impl OptionalTargetPath {
    pub fn none() -> Self {
        Self { path: None }
    }
}

impl TryFrom<String> for OptionalTargetPath {
    type Error = PathParseError;

    fn try_from(src: String) -> Result<Self, Self::Error> {
        if src.is_empty() {
            Ok(Self { path: None })
        } else {
            OwnedTargetPath::try_from(src).map(|path| Self { path: Some(path) })
        }
    }
}

impl From<OptionalTargetPath> for String {
    fn from(optional_path: OptionalTargetPath) -> Self {
        match optional_path.path {
            Some(path) => String::from(path),
            None => String::new(),
        }
    }
}

impl From<OwnedTargetPath> for OptionalTargetPath {
    fn from(path: OwnedTargetPath) -> Self {
        Self { path: Some(path) }
    }
}

#[configurable_component]
#[derive(Debug, Clone, PartialEq, Eq, Default, Hash, PartialOrd, Ord)]
#[serde(try_from = "String", into = "String")]
/// An optional path that deserializes an empty string to `None`.
pub struct OptionalValuePath {
    pub path: Option<OwnedValuePath>,
}

impl OptionalValuePath {
    pub fn none() -> Self {
        Self { path: None }
    }
}

impl TryFrom<String> for OptionalValuePath {
    type Error = PathParseError;

    fn try_from(src: String) -> Result<Self, Self::Error> {
        if src.is_empty() {
            Ok(Self { path: None })
        } else {
            OwnedValuePath::try_from(src).map(|path| Self { path: Some(path) })
        }
    }
}

impl From<OptionalValuePath> for String {
    fn from(optional_path: OptionalValuePath) -> Self {
        match optional_path.path {
            Some(path) => String::from(path),
            None => String::new(),
        }
    }
}

impl From<OwnedValuePath> for OptionalValuePath {
    fn from(path: OwnedValuePath) -> Self {
        Self { path: Some(path) }
    }
}
