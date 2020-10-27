use super::prelude::*;
use super::util::round_to_precision;

#[derive(Debug)]
pub(in crate::mapping) struct RoundFn {
    query: Box<dyn Function>,
    precision: Option<Box<dyn Function>>,
}

impl RoundFn {
    #[cfg(test)]
    pub(in crate::mapping) fn new(
        query: Box<dyn Function>,
        precision: Option<Box<dyn Function>>,
    ) -> Self {
        Self { query, precision }
    }
}

impl Function for RoundFn {
    fn execute(&self, ctx: &Event) -> Result<QueryValue> {
        let precision = optional_value!(ctx, self.precision, Value::Integer(v) => v).unwrap_or(0);
        let res = required_value!(ctx, self.query,
                            Value::Float(f) => {
                                Value::Float(round_to_precision(f, precision, f64::round))
                            },
                            v@Value::Integer(_) => v
        );

        Ok(res.into())
    }

    fn parameters() -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                accepts: |v| matches!(v, QueryValue::Value(Value::Float(_)) | QueryValue::Value(Value::Integer(_))),
                required: true,
            },
            Parameter {
                keyword: "precision",
                accepts: |v| matches!(v, QueryValue::Value(Value::Integer(_))),
                required: false,
            },
        ]
    }
}

impl TryFrom<ArgumentList> for RoundFn {
    type Error = String;

    fn try_from(mut arguments: ArgumentList) -> Result<Self> {
        let query = arguments.required("value")?;
        let precision = arguments.optional("precision");

        Ok(Self { query, precision })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapping::query::path::Path;

    #[test]
    fn round() {
        let cases = vec![
            (
                Event::from(""),
                Err("path .foo not found in event".to_string()),
                RoundFn::new(Box::new(Path::from(vec![vec!["foo"]])), None),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo", Value::from(1234.5));
                    event
                },
                Ok(Value::from(1235.0)),
                RoundFn::new(Box::new(Path::from(vec![vec!["foo"]])), None),
            ),
            (
                Event::from(""),
                Ok(Value::from(1235.0)),
                RoundFn::new(Box::new(Literal::from(Value::Float(1234.8))), None),
            ),
            (
                Event::from(""),
                Ok(Value::from(1234.0)),
                RoundFn::new(Box::new(Literal::from(Value::Float(1234.4))), None),
            ),
            (
                Event::from(""),
                Ok(Value::from(1234)),
                RoundFn::new(Box::new(Literal::from(Value::Integer(1234))), None),
            ),
            (
                Event::from(""),
                Ok(Value::from(1234.3)),
                RoundFn::new(
                    Box::new(Literal::from(Value::Float(1234.33429))),
                    Some(Box::new(Literal::from(Value::from(1)))),
                ),
            ),
            (
                Event::from(""),
                Ok(Value::from(1234.4)),
                RoundFn::new(
                    Box::new(Literal::from(Value::Float(1234.39429))),
                    Some(Box::new(Literal::from(Value::from(1)))),
                ),
            ),
            (
                Event::from(""),
                Ok(Value::from(3.1416)),
                RoundFn::new(
                    Box::new(Literal::from(Value::Float(std::f64::consts::PI))),
                    Some(Box::new(Literal::from(Value::from(4)))),
                ),
            ),
            (
                Event::from(""),
                Ok(Value::from(
                    9876543210123456789098765432101234567890987654321.98765,
                )),
                RoundFn::new(
                    Box::new(Literal::from(Value::Float(
                        9876543210123456789098765432101234567890987654321.987654321,
                    ))),
                    Some(Box::new(Literal::from(Value::from(5)))),
                ),
            ),
        ];

        for (input_event, exp, query) in cases {
            assert_eq!(query.execute(&input_event), exp.map(QueryValue::Value));
        }
    }
}
