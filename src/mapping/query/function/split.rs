use super::prelude::*;

#[derive(Debug)]
pub(in crate::mapping) struct SplitFn {
    path: Box<dyn Function>,
    pattern: ArgumentKind,
    limit: Option<Box<dyn Function>>,
}

impl SplitFn {
    #[cfg(test)]
    pub(in crate::mapping) fn new(
        path: Box<dyn Function>,
        pattern: ArgumentKind,
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

        match &self.pattern {
            ArgumentKind::Value(path) => {
                let pattern = required!(ctx, path, Value::Bytes(v) => v);
                let pattern = String::from_utf8_lossy(&pattern).into_owned();

                let iter: Box<dyn Iterator<Item = _>> = match limit {
                    Some(limit) => Box::new(string.splitn(limit, &pattern)),
                    None => Box::new(string.split(&pattern)),
                };

                Ok(Value::Array(
                    iter.map(|sub| Value::Bytes(sub.to_string().into()))
                        .collect(),
                ))
            }
            ArgumentKind::Regex(regex) => {
                // The global flag has no meaning here.
                // Should we error or just ignore?
                let iter: Box<dyn Iterator<Item = _>> = match &limit {
                    Some(ref limit) => Box::new(regex.regex.splitn(&string, *limit)),
                    None => Box::new(regex.regex.split(&string)),
                };

                Ok(Value::Array(
                    iter.map(|sub| Value::Bytes(sub.to_string().into()))
                        .collect(),
                ))
            }
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
        let path = arguments.required("value")?.into_value()?;
        let pattern = arguments.required("pattern")?;

        let limit = arguments
            .optional("limit")
            .map(|v| v.into_value())
            .transpose()?;

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
                    ArgumentKind::Value(Box::new(Literal::from(Value::from(" ")))),
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
                    ArgumentKind::Value(Box::new(Literal::from(Value::from(" ")))),
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
                    ArgumentKind::Regex(RemapRegex {
                        regex: regex::Regex::new(" ").unwrap(),
                        global: true,
                    }),
                    Some(Box::new(Literal::from(Value::from(2)))),
                ),
            ),
        ];

        for (input_event, exp, query) in cases {
            assert_eq!(query.execute(&input_event), exp);
        }
    }
}
