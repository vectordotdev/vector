use super::prelude::*;

#[derive(Debug)]
pub(in crate::mapping) struct SliceFn {
    query: Box<dyn Function>,
    start: Box<dyn Function>,
    end: Option<Box<dyn Function>>,
}

impl SliceFn {
    #[cfg(test)]
    pub(in crate::mapping) fn new(
        query: Box<dyn Function>,
        start: isize,
        end: Option<isize>,
    ) -> Self {
        let start = Box::new(Literal::from(Value::from(start as i64)));
        let end = end.map(|i| Box::new(Literal::from(Value::from(i as i64))) as _);

        Self { query, start, end }
    }
}

impl Function for SliceFn {
    fn execute(&self, ctx: &Event) -> Result<Value> {
        let mut start = match self.start.execute(ctx)? {
            Value::Integer(v) => v,
            v => unexpected_type!(v),
        };

        let end = &self
            .end
            .as_ref()
            .map(|v| v.execute(ctx))
            .transpose()?
            .map(|v| match v {
                Value::Integer(v) => v,
                v => unexpected_type!(v),
            });

        let mut range = |len: i64| {
            if start < 0 {
                start += len;
            }

            let mut end = match end {
                Some(end) => *end,
                None => len,
            };

            if end < 0 {
                end += len;
            }

            if start < 0 || start >= len {
                return Err(format!(
                    "'start' must be between '{}' and '{}'",
                    -len,
                    len - 1
                ));
            }

            if end > len {
                return Err(format!("'end' must not be greater than '{}'", len));
            }

            if end <= start {
                return Err("'end' must be greater than 'start'".to_owned());
            }

            Ok(start as usize..end as usize)
        };

        match self.query.execute(ctx)? {
            Value::Bytes(v) => range(v.len() as i64)
                .map(|range| v.slice(range))
                .map(Value::from),
            Value::Array(mut v) => range(v.len() as i64)
                .map(|range| v.drain(range).collect::<Vec<_>>())
                .map(Value::from),
            v => unexpected_type!(v),
        }
    }

    fn parameters() -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                accepts: |v| matches!(v, Value::Bytes(_) | Value::Array(_)),
                required: true,
            },
            Parameter {
                keyword: "start",
                accepts: |v| matches!(v, Value::Integer(_)),
                required: true,
            },
            Parameter {
                keyword: "end",
                accepts: |v| matches!(v, Value::Integer(_)),
                required: false,
            },
        ]
    }
}

impl TryFrom<ArgumentList> for SliceFn {
    type Error = String;

    fn try_from(mut arguments: ArgumentList) -> Result<Self> {
        let query = arguments.required("value")?;
        let start = arguments.required("start")?;
        let end = arguments.optional("end");

        Ok(Self { query, start, end })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapping::query::path::Path;

    #[test]
    fn bytes() {
        let cases = vec![
            (
                Event::from(""),
                Ok(Value::from("foo")),
                SliceFn::new(Box::new(Literal::from(Value::from("foo"))), 0, None),
            ),
            (
                Event::from(""),
                Ok(Value::from("oo")),
                SliceFn::new(Box::new(Literal::from(Value::from("foo"))), 1, None),
            ),
            (
                Event::from(""),
                Ok(Value::from("o")),
                SliceFn::new(Box::new(Literal::from(Value::from("foo"))), 2, None),
            ),
            (
                Event::from(""),
                Ok(Value::from("oo")),
                SliceFn::new(Box::new(Literal::from(Value::from("foo"))), -2, None),
            ),
            (
                Event::from(""),
                Ok(Value::from("docious")),
                SliceFn::new(
                    Box::new(Literal::from(Value::from(
                        "Supercalifragilisticexpialidocious",
                    ))),
                    -7,
                    None,
                ),
            ),
            (
                Event::from(""),
                Ok(Value::from("cali")),
                SliceFn::new(
                    Box::new(Literal::from(Value::from(
                        "Supercalifragilisticexpialidocious",
                    ))),
                    5,
                    Some(9),
                ),
            ),
        ];

        for (input_event, exp, query) in cases {
            assert_eq!(query.execute(&input_event), exp);
        }
    }

    #[test]
    fn array() {
        let cases = vec![
            (
                Event::from(""),
                Ok(Value::from(vec![0, 1, 2])),
                SliceFn::new(Box::new(Literal::from(Value::from(vec![0, 1, 2]))), 0, None),
            ),
            (
                Event::from(""),
                Ok(Value::from(vec![1, 2])),
                SliceFn::new(Box::new(Literal::from(Value::from(vec![0, 1, 2]))), 1, None),
            ),
            (
                Event::from(""),
                Ok(Value::from(vec![1, 2])),
                SliceFn::new(
                    Box::new(Literal::from(Value::from(vec![0, 1, 2]))),
                    -2,
                    None,
                ),
            ),
            (
                Event::from(""),
                Ok(Value::from("docious".bytes().collect::<Vec<_>>())),
                SliceFn::new(
                    Box::new(Literal::from(Value::from(
                        "Supercalifragilisticexpialidocious"
                            .bytes()
                            .collect::<Vec<_>>(),
                    ))),
                    -7,
                    None,
                ),
            ),
            (
                Event::from(""),
                Ok(Value::from("cali".bytes().collect::<Vec<_>>())),
                SliceFn::new(
                    Box::new(Literal::from(Value::from(
                        "Supercalifragilisticexpialidocious"
                            .bytes()
                            .collect::<Vec<_>>(),
                    ))),
                    5,
                    Some(9),
                ),
            ),
        ];

        for (input_event, exp, query) in cases {
            assert_eq!(query.execute(&input_event), exp);
        }
    }

    #[test]
    fn errors() {
        let cases = vec![
            (
                Event::from(""),
                Err("path .foo not found in event".to_string()),
                SliceFn::new(Box::new(Path::from(vec![vec!["foo"]])), 0, None),
            ),
            (
                Event::from(""),
                Err("'start' must be between '-3' and '2'".to_owned()),
                SliceFn::new(Box::new(Literal::from(Value::from("foo"))), 3, None),
            ),
            (
                Event::from(""),
                Err("'start' must be between '-3' and '2'".to_owned()),
                SliceFn::new(Box::new(Literal::from(Value::from("foo"))), -4, None),
            ),
            (
                Event::from(""),
                Err("'end' must not be greater than '3'".to_owned()),
                SliceFn::new(Box::new(Literal::from(Value::from("foo"))), 0, Some(4)),
            ),
            (
                Event::from(""),
                Err("'end' must be greater than 'start'".to_owned()),
                SliceFn::new(Box::new(Literal::from(Value::from("foo"))), 2, Some(2)),
            ),
            (
                Event::from(""),
                Err("'end' must be greater than 'start'".to_owned()),
                SliceFn::new(Box::new(Literal::from(Value::from("foo"))), 2, Some(1)),
            ),
        ];

        for (input_event, exp, query) in cases {
            assert_eq!(query.execute(&input_event), exp);
        }
    }
}
