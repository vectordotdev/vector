//! This was copied from VRL code, to make the Vector Value more similar to the VRL value.
//! Both copies will eventually be merged when the value types are merged

use bytes::Bytes;
use std::cmp::Ordering;
use std::{
    hash::{Hash, Hasher},
    ops::Deref,
};

#[derive(Debug, Clone)]
pub struct ValueRegex(regex::Regex);

impl ValueRegex {
    pub fn new(regex: regex::Regex) -> Self {
        Self(regex)
    }

    pub fn as_bytes(&self) -> Bytes {
        bytes::Bytes::copy_from_slice(self.as_bytes_slice())
    }

    pub fn as_bytes_slice(&self) -> &[u8] {
        self.as_str().as_bytes()
    }

    pub fn into_inner(self) -> regex::Regex {
        self.0
    }
}

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

impl From<regex::Regex> for ValueRegex {
    fn from(regex: regex::Regex) -> Self {
        Self(regex)
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
