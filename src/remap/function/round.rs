use super::round_to_precision;
use remap::prelude::*;

#[derive(Debug)]
pub struct Round;

impl Function for Round {
    fn identifier(&self) -> &'static str {
        "round"
    }

    fn parameters(&self) -> &'static [Parameter] {
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

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required_expr("value")?;
        let precision = arguments.optional_expr("precision")?;

        Ok(Box::new(RoundFn { value, precision }))
    }
}

#[derive(Debug)]
struct RoundFn {
    value: Box<dyn Expression>,
    precision: Option<Box<dyn Expression>>,
}

impl RoundFn {
    #[cfg(test)]
    fn new(value: Box<dyn Expression>, precision: Option<Box<dyn Expression>>) -> Self {
        Self { value, precision }
    }
}

impl Expression for RoundFn {
    fn execute(&self, state: &mut State, object: &mut dyn Object) -> Result<Option<Value>> {
        let precision =
            optional!(state, object, self.precision, Value::Integer(v) => v).unwrap_or(0);
        let res = required!(state, object, self.value,
                            Value::Float(f) => {
                                Value::Float(round_to_precision(f, precision, f64::round))
                            },
                            v@Value::Integer(_) => v
        );

        Ok(res.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map;

    #[test]
    fn round() {
        let cases = vec![
            (
                map![],
                Err("path error: missing path: foo".into()),
                RoundFn::new(Box::new(Path::from("foo")), None),
            ),
            (
                map!["foo": 1234.2],
                Ok(Some(1234.0.into())),
                RoundFn::new(Box::new(Path::from("foo")), None),
            ),
            (
                map![],
                Ok(Some(1235.0.into())),
                RoundFn::new(Box::new(Literal::from(Value::Float(1234.8))), None),
            ),
            (
                map![],
                Ok(Some(1234.into())),
                RoundFn::new(Box::new(Literal::from(Value::Integer(1234))), None),
            ),
            (
                map![],
                Ok(Some(1234.4.into())),
                RoundFn::new(
                    Box::new(Literal::from(Value::Float(1234.39429))),
                    Some(Box::new(Literal::from(1))),
                ),
            ),
            (
                map![],
                Ok(Some(3.1416.into())),
                RoundFn::new(
                    Box::new(Literal::from(Value::Float(std::f64::consts::PI))),
                    Some(Box::new(Literal::from(4))),
                ),
            ),
            (
                map![],
                Ok(Some(
                    9876543210123456789098765432101234567890987654321.98765.into(),
                )),
                RoundFn::new(
                    Box::new(Literal::from(
                        9876543210123456789098765432101234567890987654321.987654321,
                    )),
                    Some(Box::new(Literal::from(5))),
                ),
            ),
        ];

        let mut state = remap::State::default();

        for (mut object, exp, func) in cases {
            let got = func
                .execute(&mut state, &mut object)
                .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));

            assert_eq!(got, exp);
        }
    }
}
