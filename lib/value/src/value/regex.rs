use std::cmp::Ordering;
use std::{
    hash::{Hash, Hasher},
    ops::Deref,
};

/// A wrapper around a Regex to add custom Hasing / Ordering / Eq, etc
#[derive(Debug, Clone)]
pub struct ValueRegex(regex::Regex);

impl ValueRegex {
    /// Returns the raw Regex type that is being wrapped
    #[must_use] pub fn into_inner(self) -> regex::Regex {
        self.0
    }
}

impl PartialEq for ValueRegex {
    fn eq(&self, other: &Self) -> bool {
        self.0.as_str() == other.0.as_str()
    }
}

impl Eq for ValueRegex {}

impl Hash for ValueRegex {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.as_str().hash(state)
    }
}

impl Deref for ValueRegex {
    type Target = regex::Regex;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl PartialOrd for ValueRegex {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.as_str().partial_cmp(other.as_str())
    }
}

impl From<regex::Regex> for ValueRegex {
    fn from(regex: regex::Regex) -> Self {
        Self(regex)
    }
}
