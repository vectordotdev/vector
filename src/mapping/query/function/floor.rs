use super::prelude::*;
use super::util::round_to_precision;

#[derive(Debug)]
pub(in crate::mapping) struct FloorFn {
    query: Box<dyn Function>,
    precision: Option<Box<dyn Function>>,
}

impl FloorFn {
    #[cfg(test)]
    pub(in crate::mapping) fn new(
        query: Box<dyn Function>,
        precision: Option<Box<dyn Function>>,
    ) -> Self {
        Self { query, precision }
    }
}

impl Function for FloorFn {
    fn execute(&self, ctx: &Event) -> Result<Value> {
        let precision = optional!(ctx, self.precision, Value::Integer(v) => v).unwrap_or(0);
        let res = required!(ctx, self.query,
                            Value::Float(f) => {
                                Value::Float(round_to_precision(f, precision, f64::floor))
                            },
                            v@Value::Integer(_) => v
        );

        Ok(res)
    }

    fn parameters() -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                accepts: |v| matches!(v, Value::Float(_) | Value::Integer(_)),
                required: true,
            },
            Parameter {
                keyword: "precision",
                accepts: |v| matches!(v, Value::Integer(_)),
                required: false,
            },
        ]
    }
}

impl TryFrom<ArgumentList> for FloorFn {
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
    fn floor() {
        let cases = vec![
            (
                Event::from(""),
                Err("path .foo not found in event".to_string()),
                FloorFn::new(Box::new(Path::from(vec![vec!["foo"]])), None),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo", Value::from(1234.2));
                    event
                },
                Ok(Value::from(1234.0)),
                FloorFn::new(Box::new(Path::from(vec![vec!["foo"]])), None),
            ),
            (
                Event::from(""),
                Ok(Value::from(1234.0)),
                FloorFn::new(Box::new(Literal::from(Value::Float(1234.8))), None),
            ),
            (
                Event::from(""),
                Ok(Value::from(1234)),
                FloorFn::new(Box::new(Literal::from(Value::Integer(1234))), None),
            ),
            (
                Event::from(""),
                Ok(Value::from(1234.3)),
                FloorFn::new(
                    Box::new(Literal::from(Value::Float(1234.39429))),
                    Some(Box::new(Literal::from(Value::from(1)))),
                ),
            ),
            (
                Event::from(""),
                Ok(Value::from(3.1415)),
                FloorFn::new(
                    Box::new(Literal::from(Value::Float(std::f64::consts::PI))),
                    Some(Box::new(Literal::from(Value::from(4)))),
                ),
            ),
            (
                Event::from(""),
                Ok(Value::from(
                    9876543210123456789098765432101234567890987654321.98765,
                )),
                FloorFn::new(
                    Box::new(Literal::from(Value::Float(
                        9876543210123456789098765432101234567890987654321.987654321,
                    ))),
                    Some(Box::new(Literal::from(Value::from(5)))),
                ),
            ),
        ];

        for (input_event, exp, query) in cases {
            assert_eq!(query.execute(&input_event), exp);
        }
    }
}
