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
    fn execute(&self, ctx: &Event) -> Result<QueryValue> {
        let range = |len: i64| {
            let start = match required_value!(ctx, self.start, Value::Integer(v) => v) {
                start if start < 0 => start + len,
                start => start,
            };

            let end = match optional_value!(ctx, self.end, Value::Integer(v) => v) {
                Some(end) if end < 0 => end + len,
                Some(end) => end,
                None => len,
            };

            match () {
                _ if start < 0 || start > len => {
                    Err(format!("'start' must be between '{}' and '{}'", -len, len))
                }
                _ if end < start => Err("'end' must be greater or equal to 'start'".to_owned()),
                _ if end > len => Ok(start as usize..len as usize),
                _ => Ok(start as usize..end as usize),
            }
        };

        required_value! {
            ctx, self.query,
            Value::Bytes(v) => range(v.len() as i64)
                .map(|range| v.slice(range))
                .map(Value::from)
                .map(Into::into),
            Value::Array(mut v) => range(v.len() as i64)
                .map(|range| v.drain(range).collect::<Vec<_>>())
                .map(Value::from)
                .map(Into::into),
        }
    }

    fn parameters() -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                accepts: |v| matches!(v, QueryValue::Value(Value::Bytes(_)) | QueryValue::Value(Value::Array(_))),
                required: true,
            },
            Parameter {
                keyword: "start",
                accepts: |v| matches!(v, QueryValue::Value(Value::Integer(_))),
                required: true,
            },
            Parameter {
                keyword: "end",
                accepts: |v| matches!(v, QueryValue::Value(Value::Integer(_))),
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
                Ok(Value::from("")),
                SliceFn::new(Box::new(Literal::from(Value::from("foo"))), 3, None),
            ),
            (
                Event::from(""),
                Ok(Value::from("")),
                SliceFn::new(Box::new(Literal::from(Value::from("foo"))), 2, Some(2)),
            ),
            (
                Event::from(""),
                Ok(Value::from("foo")),
                SliceFn::new(Box::new(Literal::from(Value::from("foo"))), 0, Some(4)),
            ),
            (
                Event::from(""),
                Ok(Value::from("oo")),
                SliceFn::new(Box::new(Literal::from(Value::from("foo"))), 1, Some(5)),
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
            assert_eq!(query.execute(&input_event), exp.map(QueryValue::Value));
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
            assert_eq!(query.execute(&input_event), exp.map(QueryValue::Value));
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
                Err("'start' must be between '-3' and '3'".to_owned()),
                SliceFn::new(Box::new(Literal::from(Value::from("foo"))), 4, None),
            ),
            (
                Event::from(""),
                Err("'start' must be between '-3' and '3'".to_owned()),
                SliceFn::new(Box::new(Literal::from(Value::from("foo"))), -4, None),
            ),
            (
                Event::from(""),
                Err("'end' must be greater or equal to 'start'".to_owned()),
                SliceFn::new(Box::new(Literal::from(Value::from("foo"))), 2, Some(1)),
            ),
        ];

        for (input_event, exp, query) in cases {
            assert_eq!(query.execute(&input_event), exp);
        }
    }
}
