use std::ops::Deref;

#[derive(Debug, Clone)]
pub struct Regex(regex::Regex);

impl PartialEq for Regex {
    fn eq(&self, other: &Self) -> bool {
        self.0.as_str() == other.0.as_str()
    }
}

impl Deref for Regex {
    type Target = regex::Regex;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<regex::Regex> for Regex {
    fn from(regex: regex::Regex) -> Self {
        Self(regex)
    }
}
