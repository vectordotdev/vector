use super::prelude::*;
use bytes::Bytes;
use std::collections::BTreeMap;
use std::convert::TryFrom;

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

#[derive(Debug)]
pub(in crate::mapping) struct SplitFn {
    path: Box<dyn Function>,
    pattern: Box<dyn Function>,
    limit: Option<Box<dyn Function>>,
}

impl SplitFn {
    #[cfg(test)]
    pub(in crate::mapping) fn new(
        path: Box<dyn Function>,
        pattern: Box<dyn Function>,
        limit: Option<Box<dyn Function>>,
    ) -> Self {
        Self {
            path,
            pattern,
            limit,
        }
    }
}

impl Function for SplitFn {
    fn execute(&self, ctx: &Event) -> Result<Value> {
        let string = {
            let bytes = required!(ctx, self.path, Value::Bytes(v) => v);
            String::from_utf8_lossy(&bytes).into_owned()
        };

        let limit = optional!(ctx, self.limit, Value::Integer(i) => i)
            .map(|limit| {
                if limit < 0 {
                    Err("limit is not a positive number".to_string())
                } else {
                    Ok(limit as usize)
                }
            })
            .transpose()?;

        let to_value = |iter: Box<dyn Iterator<Item = &str>>| {
            Value::Array(
                iter.map(|sub| Value::Bytes(sub.to_string().into()))
                    .collect(),
            )
        };

        match self.pattern.execute(ctx)? {
            Value::Bytes(path) => {
                let pattern = String::from_utf8_lossy(&path).into_owned();
                Ok(match limit {
                    Some(limit) => to_value(Box::new(string.splitn(limit, &pattern))),
                    None => to_value(Box::new(string.split(&pattern))),
                })
            }
            Value::Map(pattern) => {
                let mut regex = DynamicRegex::try_from(pattern)?;
                let regex = regex.compile()?;
                Ok(match &limit {
                    Some(ref limit) => to_value(Box::new(regex.splitn(&string, *limit))),
                    None => to_value(Box::new(regex.split(&string))),
                })
            }
            _ => Err("invalid pattern".to_string()),
        }
    }

    fn parameters() -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                accepts: |v| matches!(v, Value::Bytes(_)),
                required: true,
            },
            Parameter {
                keyword: "pattern",
                accepts: |v| matches!(v, Value::Bytes(_)) || matches!(v, Value::Map(_)),
                required: true,
            },
            Parameter {
                keyword: "limit",
                accepts: |v| matches!(v, Value::Integer(_)),
                required: false,
            },
        ]
    }
}

impl TryFrom<ArgumentList> for SplitFn {
    type Error = String;

    fn try_from(mut arguments: ArgumentList) -> Result<Self> {
        let path = arguments.required("value")?;
        let pattern = arguments.required("pattern")?;
        let limit = arguments.optional("limit");

        Ok(Self {
            path,
            pattern,
            limit,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapping::query::path::Path;

    #[test]
    fn check_split() {
        let cases = vec![
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo", Value::from(""));
                    event
                },
                Ok(Value::from(vec![Value::from("")])),
                SplitFn::new(
                    Box::new(Path::from(vec![vec!["foo"]])),
                    Box::new(Literal::from(Value::from(" "))),
                    None,
                ),
            ),
            (
                {
                    let mut event = Event::from("");
                    event
                        .as_mut_log()
                        .insert("foo", Value::from("This is a long string."));
                    event
                },
                Ok(Value::from(vec![
                    Value::from("This"),
                    Value::from("is"),
                    Value::from("a"),
                    Value::from("long"),
                    Value::from("string."),
                ])),
                SplitFn::new(
                    Box::new(Path::from(vec![vec!["foo"]])),
                    Box::new(Literal::from(Value::from(" "))),
                    None,
                ),
            ),
            (
                {
                    let mut event = Event::from("");
                    event
                        .as_mut_log()
                        .insert("foo", Value::from("This is a long string."));
                    event
                },
                Ok(Value::from(vec![
                    Value::from("This"),
                    Value::from("is a long string."),
                ])),
                SplitFn::new(
                    Box::new(Path::from(vec![vec!["foo"]])),
                    Box::new(Literal::from(Value::from({
                        let mut map = BTreeMap::new();
                        map.insert("pattern".to_string(), Value::from(" "));
                        map
                    }))),
                    Some(Box::new(Literal::from(Value::from(2)))),
                ),
            ),
            (
                {
                    let mut event = Event::from("");
                    event
                        .as_mut_log()
                        .insert("foo", Value::from("ThisaisAlongAstring."));
                    event
                },
                Ok(Value::from(vec![
                    Value::from("This"),
                    Value::from("is"),
                    Value::from("long"),
                    Value::from("string."),
                ])),
                SplitFn::new(
                    Box::new(Path::from(vec![vec!["foo"]])),
                    Box::new(Literal::from(Value::from({
                        let mut map = BTreeMap::new();
                        map.insert("pattern".to_string(), Value::from("a"));
                        map.insert("flags".to_string(), Value::from(vec![Value::from("i")]));
                        map
                    }))),
                    None,
                ),
            ),
        ];

        for (input_event, exp, query) in cases {
            assert_eq!(query.execute(&input_event), exp);
        }
    }
}
