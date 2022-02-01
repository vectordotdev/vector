use std::{
    hash::{Hash, Hasher},
    ops::Deref,
};

#[derive(Debug, Clone)]
pub struct VrlRegex(regex::Regex);

impl VrlRegex {
    pub fn into_inner(self) -> regex::Regex {
        self.0
    }
}

impl PartialEq for VrlRegex {
    fn eq(&self, other: &Self) -> bool {
        self.0.as_str() == other.0.as_str()
    }
}

impl Hash for VrlRegex {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.as_str().hash(state)
    }
}

impl Deref for VrlRegex {
    type Target = regex::Regex;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<regex::Regex> for VrlRegex {
    fn from(regex: regex::Regex) -> Self {
        Self(regex)
    }
}
