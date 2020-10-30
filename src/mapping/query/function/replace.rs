use super::prelude::*;

#[derive(Debug)]
pub(in crate::mapping) struct ReplaceFn {
    value: Box<dyn Function>,
    pattern: Box<dyn Function>,
    with: Box<dyn Function>,
}

impl ReplaceFn {
    #[cfg(test)]
    pub(in crate::mapping) fn new(
        value: Box<dyn Function>,
        pattern: Box<dyn Function>,
        with: Box<dyn Function>,
    ) -> Self {
        ReplaceFn {
            value,
            pattern,
            with,
        }
    }
}

impl Function for ReplaceFn {
    fn execute(&self, ctx: &Event) -> Result<QueryValue> {
        let value = required_value!(ctx, self.value, Value::Bytes(v) => String::from_utf8_lossy(&v).into_owned());
        let with = required_value!(ctx, self.with, Value::Bytes(v) => String::from_utf8_lossy(&v).into_owned());

        match self.pattern.execute(ctx)? {
            QueryValue::Value(Value::Bytes(path)) => {
                let pattern = String::from_utf8_lossy(&path).into_owned();
                let replaced = value.replace(&pattern, &with);
                Ok(Value::Bytes(replaced.into()).into())
            }
            QueryValue::Regex(regex) => {
                let replaced = if regex.is_global() {
                    regex.regex().replace_all(&value, with.as_str())
                } else {
                    regex.regex().replace(&value, with.as_str())
                };

                Ok(Value::Bytes(replaced.into_owned().into()).into())
            }
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
                accepts: |v| {
                    matches!(v, QueryValue::Value(Value::Bytes(_))
                             | QueryValue::Regex(_))
                },
                required: true,
            },
            Parameter {
                keyword: "with",
                accepts: |v| matches!(v, QueryValue::Value(Value::Bytes(_))),
                required: true,
            },
        ]
    }
}

impl TryFrom<ArgumentList> for ReplaceFn {
    type Error = String;

    fn try_from(mut arguments: ArgumentList) -> Result<Self> {
        let value = arguments.required("value")?;
        let pattern = arguments.required("pattern")?;
        let with = arguments.required("with")?;

        Ok(Self {
            value,
            pattern,
            with,
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::mapping::query::path::Path;
    use crate::mapping::query::regex::Regex;

    #[test]
    fn check_replace() {
        let cases = vec![
            (
                {
                    let mut event = Event::from("");
                    event
                        .as_mut_log()
                        .insert("foo", Value::from("I like apples and bananas"));
                    event
                },
                Ok(Value::from("I like opples ond bononos")),
                ReplaceFn::new(
                    Box::new(Path::from(vec![vec!["foo"]])),
                    Box::new(Literal::from(QueryValue::from(
                        Regex::new("a".to_string(), false, false, true).unwrap(),
                    ))),
                    Box::new(Literal::from(Value::from("o"))),
                ),
            ),
            (
                {
                    let mut event = Event::from("");
                    event
                        .as_mut_log()
                        .insert("foo", Value::from("I like apples and bananas"));
                    event
                },
                Ok(Value::from("I like opples and bananas")),
                ReplaceFn::new(
                    Box::new(Path::from(vec![vec!["foo"]])),
                    Box::new(Literal::from(QueryValue::from(
                        Regex::new("a".to_string(), false, false, false).unwrap(),
                    ))),
                    Box::new(Literal::from(Value::from("o"))),
                ),
            ),
            (
                {
                    let mut event = Event::from("");
                    event
                        .as_mut_log()
                        .insert("foo", Value::from("I like [apples] and bananas"));
                    event
                },
                Ok(Value::from("I like biscuits and bananas")),
                ReplaceFn::new(
                    Box::new(Path::from(vec![vec!["foo"]])),
                    Box::new(Literal::from(QueryValue::from(
                        Regex::new("\\[apples\\]".to_string(), false, false, true).unwrap(),
                    ))),
                    Box::new(Literal::from(Value::from("biscuits"))),
                ),
            ),
        ];

        for (input_event, exp, query) in cases {
            assert_eq!(query.execute(&input_event), exp.map(QueryValue::Value));
        }
    }
}
