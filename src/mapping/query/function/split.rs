use super::prelude::*;
use bytes::Bytes;
use lazy_static::lazy_static;
use std::collections::BTreeMap;
use std::ops::Deref;

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

lazy_static! {
    static ref I_FLAG: Value = Value::from(Bytes::from("i"));
    static ref G_FLAG: Value = Value::from(Bytes::from("g"));
    static ref M_FLAG: Value = Value::from(Bytes::from("m"));
}

/// Create a regex from a map containing fields :
/// pattern - The regex pattern
/// flags   - flags including i => Case insensitive, g => Global, m => Multiline.
fn regex_from_map(mut map: BTreeMap<String, Value>) -> Result<(regex::Regex, bool)> {
    let pattern = map
        .remove("pattern")
        .ok_or_else(|| "Field is not a regular expression".to_string())?;

    let flags = match map.remove("flags") {
        None => Value::from(Vec::<Value>::new()),
        Some(flags) => flags,
    };

    match (flags, pattern) {
        (Value::Array(ref flags), Value::Bytes(ref bytes)) => {
            let (global, insensitive, multi_line) =
                flags
                    .iter()
                    .fold((false, false, false), |(g, i, m), flag| match flag {
                        v if v == G_FLAG.deref() => (true, i, m),
                        v if v == I_FLAG.deref() => (g, true, m),
                        v if v == M_FLAG.deref() => (g, i, true),
                        _ => (g, i, m),
                    });

            let pattern = String::from_utf8_lossy(&bytes);
            let regex = regex::RegexBuilder::new(&pattern)
                .case_insensitive(insensitive)
                .multi_line(multi_line)
                .build()
                .map_err(|err| format!("invalid regex {}", err))?;
            Ok((regex, global))
        }
        _ => Err("Field regular expression is not a valid string".to_string()),
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
                let (regex, _global) = regex_from_map(pattern)?;
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
                accepts: |v| matches!(v, Value::Bytes(_)),
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
