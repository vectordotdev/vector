use super::regex::Regex;
use crate::event::Value;

#[derive(PartialEq, Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub(in crate::mapping) enum QueryValue {
    Value(Value),

    #[allow(dead_code)]
    Regex(Regex),
}

impl QueryValue {
    pub fn kind(&self) -> &str {
        match self {
            QueryValue::Value(value) => value.kind(),
            QueryValue::Regex(_) => "regex",
        }
    }
}

impl From<Value> for QueryValue {
    fn from(value: Value) -> Self {
        Self::Value(value)
    }
}

impl From<Regex> for QueryValue {
    fn from(value: Regex) -> Self {
        Self::Regex(value)
    }
}
