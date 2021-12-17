use super::prelude::*;

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
    #[allow(clippy::collapsible_match)] // I expect this file to be going away shortly
    #[allow(clippy::cast_possible_truncation)] // `limit as usize` might misbehave on 32bit platforms
    fn execute(&self, ctx: &Event) -> Result<QueryValue> {
        let string = {
            let bytes = required_value!(ctx, self.path, Value::Bytes(v) => v);
            String::from_utf8_lossy(&bytes).into_owned()
        };
        let limit = optional_value!(ctx, self.limit, Value::Integer(i) => i)
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
            .into()
        };

        match self.pattern.execute(ctx)? {
            QueryValue::Value(Value::Bytes(path)) => {
                let pattern = String::from_utf8_lossy(&path).into_owned();
                Ok(match limit {
                    Some(limit) => to_value(Box::new(string.splitn(limit, &pattern))),
                    None => to_value(Box::new(string.split(&pattern))),
                })
            }
            QueryValue::Regex(regex) => Ok(match &limit {
                Some(ref limit) => to_value(Box::new(regex.regex().splitn(&string, *limit))),
                None => to_value(Box::new(regex.regex().split(&string))),
            }),
            _ => Err("invalid pattern".to_string()),
        }
    }

    fn parameters() -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                accepts: |v| matches!(v, QueryValue::Value(Value::Bytes(_))),
                required: true,
            },
            Parameter {
                keyword: "pattern",
                accepts: |v| matches!(v, QueryValue::Value(Value::Bytes(_)) | QueryValue::Regex(_)),
                required: true,
            },
            Parameter {
                keyword: "limit",
                accepts: |v| matches!(v, QueryValue::Value(Value::Integer(_))),
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
    use crate::mapping::query::{path::Path, regex::Regex};

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
                    Box::new(Literal::from(QueryValue::from(
                        Regex::new(" ".to_string(), false, false, false).unwrap(),
                    ))),
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
                    Box::new(Literal::from(QueryValue::from(
                        Regex::new("a".to_string(), false, true, false).unwrap(),
                    ))),
                    None,
                ),
            ),
        ];

        for (input_event, exp, query) in cases {
            assert_eq!(query.execute(&input_event), exp.map(QueryValue::Value));
        }
    }
}
