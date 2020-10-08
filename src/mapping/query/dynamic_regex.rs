use crate::{event::Value, mapping::Result};
use bytes::Bytes;
use std::{collections::BTreeMap, convert::TryFrom};

#[derive(Debug, Clone)]
pub struct DynamicRegex {
    pattern: String,
    multiline: bool,
    insensitive: bool,
    global: bool,
    compiled: Option<Result<regex::Regex>>,
}

impl DynamicRegex {
    pub fn new(pattern: String, multiline: bool, insensitive: bool, global: bool) -> Self {
        Self {
            pattern,
            multiline,
            insensitive,
            global,
            compiled: None,
        }
    }

    #[allow(dead_code)]
    pub fn is_global(&self) -> bool {
        self.global
    }

    pub fn compile(&mut self) -> Result<&regex::Regex> {
        if self.compiled.is_none() {
            self.compiled = Some(
                regex::RegexBuilder::new(&self.pattern)
                    .case_insensitive(self.insensitive)
                    .multi_line(self.multiline)
                    .build()
                    .map_err(|err| format!("invalid regex {}", err)),
            );
        }

        self.compiled
            .as_ref()
            // We know this unwrap is safe because we have just populated the Option above.
            .unwrap()
            .as_ref()
            .map_err(|err| err.to_string())
    }
}

/// Our dynamic regex equality shouldn't rely on the compiled value
/// as this is largely an implementation detail.
/// Plus regex::Regex doesn't implement PartialEq.
impl PartialEq for DynamicRegex {
    fn eq(&self, other: &Self) -> bool {
        self.pattern == other.pattern
            && self.multiline == other.multiline
            && self.insensitive == other.insensitive
            && self.global == other.global
    }
}

impl TryFrom<BTreeMap<String, Value>> for DynamicRegex {
    type Error = String;

    /// Create a regex from a map containing fields :
    /// pattern - The regex pattern
    /// flags   - flags including i => Case insensitive, g => Global, m => Multiline.
    fn try_from(map: BTreeMap<String, Value>) -> std::result::Result<Self, Self::Error> {
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

        Ok(DynamicRegex::new(pattern, multiline, insensitive, global))
    }
}
