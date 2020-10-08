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
    fn execute(&self, ctx: &Event) -> Result<QueryValue> {
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
            ).into()
        };

        match self.pattern.execute(ctx)?.into() {
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
    use std::collections::BTreeMap;

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
            assert_eq!(query.execute(&input_event).map(Into::into), exp);
        }
    }
}
