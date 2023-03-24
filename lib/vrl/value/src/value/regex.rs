use bytes::Bytes;
use regex::Regex;
use std::cmp::Ordering;
use std::sync::Arc;
use std::{
    hash::{Hash, Hasher},
    ops::Deref,
};

/// Wraps a `Regex` and provides several trait implementations, such as `PartialOrd`
#[derive(Debug, Clone)]
pub struct ValueRegex(Arc<regex::Regex>);

impl ValueRegex {
    /// Create a new `ValueRegex` from the inner `Regex` that is wraps
    #[must_use]
    pub const fn new(regex: Arc<regex::Regex>) -> Self {
        Self(regex)
    }

    /// Returns a `Bytes` of the string representation of the regex
    #[must_use]
    pub fn as_bytes(&self) -> Bytes {
        bytes::Bytes::copy_from_slice(self.as_bytes_slice())
    }

    /// Returns a byte array of the string representation of the regex
    #[must_use]
    pub fn as_bytes_slice(&self) -> &[u8] {
        self.as_str().as_bytes()
    }

    /// Returns the inner Regex value
    #[allow(clippy::missing_const_for_fn)] // false positive
    #[must_use]
    pub fn into_inner(self) -> Arc<regex::Regex> {
        self.0
    }
}

impl Eq for ValueRegex {}

impl PartialEq for ValueRegex {
    fn eq(&self, other: &Self) -> bool {
        self.0.as_str() == other.0.as_str()
    }
}

impl Hash for ValueRegex {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.as_str().hash(state);
    }
}

impl Deref for ValueRegex {
    type Target = regex::Regex;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<Arc<regex::Regex>> for ValueRegex {
    fn from(regex: Arc<regex::Regex>) -> Self {
        Self(regex)
    }
}

impl From<Regex> for ValueRegex {
    fn from(r: Regex) -> Self {
        Self::new(Arc::new(r))
    }
}

impl PartialOrd for ValueRegex {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.0
            .as_str()
            .as_bytes()
            .partial_cmp(other.0.as_str().as_bytes())
    }
}
