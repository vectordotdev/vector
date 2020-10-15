use crate::{event::Value, mapping::Result};
use bytes::Bytes;
use std::{collections::BTreeMap, convert::TryFrom, string::ToString};

#[derive(Debug, Clone)]
pub struct Regex {
    pattern: String,
    multiline: bool,
    insensitive: bool,
    global: bool,
    compiled: regex::Regex,
}

impl Regex {
    pub fn new(pattern: String, multiline: bool, insensitive: bool, global: bool) -> Result<Self> {
        let compiled = Self::compile(&pattern, multiline, insensitive)?;
        Ok(Self {
            pattern,
            multiline,
            insensitive,
            global,
            compiled,
        })
    }

    pub fn regex(&self) -> &regex::Regex {
        &self.compiled
    }

    #[allow(dead_code)]
    pub fn is_global(&self) -> bool {
        self.global
    }

    fn compile(
        pattern: &str,
        multiline: bool,
        insensitive: bool
    ) -> Result<regex::Regex> {
        regex::RegexBuilder::new(pattern)
            .case_insensitive(insensitive)
            .multi_line(multiline)
            .build()
            .map_err(|err| format!("invalid regex {}", err))
    }

    fn from_bytes(bytes: bytes::Bytes) -> Result<Self> {
        let pattern = String::from_utf8_lossy(&bytes);
        Regex::new(pattern.to_string(), false, false, false)
    }

    fn from_map(map: BTreeMap<String, Value>) -> Result<Self> {
        let pattern = map
            .get("pattern")
            .ok_or_else(|| "field is not a regular expression".to_string())
            .and_then(|value| match value {
                Value::Bytes(ref bytes) => Ok(String::from_utf8_lossy(bytes)),
                _ => Err("regex pattern is not a valid string".to_string()),
            })?
            .to_string();

        let (global, insensitive, multiline) = match map.get("flags") {
            None => (false, false, false),
            Some(Value::Array(ref flags)) => {
                flags
                    .iter()
                    .fold((false, false, false), |(g, i, m), flag| match flag {
                        v if v == &Value::from(Bytes::from_static(b"g")) => (true, i, m),
                        v if v == &Value::from(Bytes::from_static(b"i")) => (g, true, m),
                        v if v == &Value::from(Bytes::from_static(b"m")) => (g, i, true),
                        _ => (g, i, m),
                    })
            }
            Some(_) => return Err("regular expression flags is not an array".to_string()),
        };

        Regex::new(pattern, multiline, insensitive, global)
    }
}

/// Our dynamic regex equality shouldn't rely on the compiled value
/// as this is largely an implementation detail.
/// Plus regex::Regex doesn't implement PartialEq.
impl PartialEq for Regex {
    fn eq(&self, other: &Self) -> bool {
        self.pattern == other.pattern
            && self.multiline == other.multiline
            && self.insensitive == other.insensitive
            && self.global == other.global
    }
}

impl TryFrom<Value> for Regex {
    type Error = String;

    /// Create a regex from a String or a Map containing fields :
    /// pattern - The regex pattern
    /// flags   - flags including i => Case insensitive, g => Global, m => Multiline.
    fn try_from(value: Value) -> Result<Self> {
        match value {
            Value::Map(map) => Regex::from_map(map),
            Value::Bytes(bytes) => Regex::from_bytes(bytes),
            _ => Err("regular expression should be a map or a string".to_string()),
        }
    }
}

impl From<Regex> for Value {
    fn from(regex: Regex) -> Self {
        let mut map = BTreeMap::new();
        map.insert("pattern".to_string(), Value::from(regex.pattern));

        let mut flags = Vec::new();
        if regex.insensitive {
            flags.push(Value::from(Bytes::from_static(b"i")));
        }
        if regex.multiline {
            flags.push(Value::from(Bytes::from_static(b"m")));
        }
        if regex.global {
            flags.push(Value::from(Bytes::from_static(b"g")));
        }

        map.insert("flags".to_string(), Value::from(flags));

        Value::from(map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_regex() {
        let regex = Regex::new("abba".to_string(), false, false, false).unwrap();

        // Test our regex is working case sensitively.
        assert!(regex.regex().is_match("abba"));
    }

    #[test]
    fn create_regex_from_string() {
        let value = Value::from("abba");
        let regex = Regex::try_from(value).unwrap();

        // Test our regex is working case sensitively.
        assert!(regex.regex().is_match("abba"));
        assert!(!regex.regex().is_match("aBBa"));
    }

    #[test]
    fn create_regex_from_map() {
        let mut map = BTreeMap::new();
        map.insert("pattern".to_string(), Value::from("abba"));
        map.insert("flags".to_string(), Value::from(vec![Value::from("i")]));
        let value = Value::from(map);

        let regex = Regex::try_from(value).unwrap();

        // Test our regex is working case insensitively.
        assert!(regex.regex().is_match("AbBa"));
    }
}
