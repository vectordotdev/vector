use crate::types::Conversion;
use chrono::{TimeZone, Utc};
use remap::prelude::*;

#[derive(Debug)]
pub struct ToTimestamp;

impl Function for ToTimestamp {
    fn identifier(&self) -> &'static str {
        "to_timestamp"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                accepts: |v| {
                    matches!(
                        v,
                        Value::Integer(_) |
                        Value::Float(_) |
                        Value::String(_) |
                        Value::Timestamp(_)
                    )
                },
                required: true,
            },
            Parameter {
                keyword: "default",
                accepts: |v| {
                    matches!(
                        v,
                        Value::Integer(_) |
                        Value::Float(_) |
                        Value::String(_) |
                        Value::Timestamp(_)
                    )
                },
                required: false,
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required_expr("value")?;
        let default = arguments.optional_expr("default")?;

        Ok(Box::new(ToTimestampFn { value, default }))
    }
}

#[derive(Debug)]
struct ToTimestampFn {
    value: Box<dyn Expression>,
    default: Option<Box<dyn Expression>>,
}

impl ToTimestampFn {
    #[cfg(test)]
    fn new(value: Box<dyn Expression>, default: Option<Value>) -> Self {
        let default = default.map(|v| Box::new(Literal::from(v)) as _);
        Self { value, default }
    }
}

impl Expression for ToTimestampFn {
    fn execute(&self, state: &mut State, object: &mut dyn Object) -> Result<Option<Value>> {
        use Value::*;

        let to_timestamp = |value| match value {
            Timestamp(_) => Ok(value),
            String(_) => Conversion::Timestamp
                .convert(value.into())
                .map(Into::into)
                .map_err(|e| e.to_string().into()),
            Integer(v) => Ok(Timestamp(Utc.timestamp(v, 0))),
            Float(v) => Ok(Timestamp(Utc.timestamp(v.round() as i64, 0))),
            _ => Err("unable to convert value to timestamp".into()),
        };

        super::convert_value_or_default(
            self.value.execute(state, object),
            self.default.as_ref().map(|v| v.execute(state, object)),
            to_timestamp,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map;

    #[test]
    fn to_timestamp() {
        let cases = vec![
            (
                map![],
                Err("path error: missing path: foo".into()),
                ToTimestampFn::new(Box::new(Path::from("foo")), None),
            ),
            (
                map![],
                Ok(Some(Utc.timestamp(10, 0).into())),
                ToTimestampFn::new(Box::new(Path::from("foo")), Some(10.into())),
            ),
            (
                map![],
                Ok(Some(Utc.timestamp(10, 0).into())),
                ToTimestampFn::new(
                    Box::new(Path::from("foo")),
                    Some(Utc.timestamp(10, 0).into()),
                ),
            ),
            (
                map![],
                Ok(Some(Value::Timestamp(Utc.timestamp(10, 0)))),
                ToTimestampFn::new(Box::new(Path::from("foo")), Some("10".into())),
            ),
            (
                map!["foo": Utc.timestamp(10, 0)],
                Ok(Some(Value::Timestamp(Utc.timestamp(10, 0)))),
                ToTimestampFn::new(Box::new(Path::from("foo")), None),
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
