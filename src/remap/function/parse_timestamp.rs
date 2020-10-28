use crate::types::Conversion;
use remap::prelude::*;

#[derive(Debug)]
pub struct ParseTimestamp;

impl Function for ParseTimestamp {
    fn identifier(&self) -> &'static str {
        "parse_timestamp"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                accepts: |v| matches!(v, Value::String(_) | Value::Timestamp(_)),
                required: true,
            },
            Parameter {
                keyword: "format",
                accepts: |v| matches!(v, Value::String(_)),
                required: true,
            },
            Parameter {
                keyword: "default",
                accepts: |v| matches!(v, Value::String(_) | Value::Timestamp(_)),
                required: false,
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required_expr("value")?;
        let format = arguments.required_expr("format")?;
        let default = arguments.optional_expr("default")?;

        Ok(Box::new(ParseTimestampFn {
            value,
            format,
            default,
        }))
    }
}

#[derive(Debug)]
struct ParseTimestampFn {
    value: Box<dyn Expression>,
    format: Box<dyn Expression>,
    default: Option<Box<dyn Expression>>,
}

impl ParseTimestampFn {
    #[cfg(test)]
    fn new(format: &str, value: Box<dyn Expression>, default: Option<Value>) -> Self {
        let format = Box::new(Literal::from(format));
        let default = default.map(|v| Box::new(Literal::from(v)) as _);

        Self {
            value,
            format,
            default,
        }
    }
}

impl Expression for ParseTimestampFn {
    fn execute(&self, state: &mut State, object: &mut dyn Object) -> Result<Option<Value>> {
        let format = {
            let bytes = required!(state, object, self.format, Value::String(v) => v);
            format!("timestamp|{}", String::from_utf8_lossy(&bytes))
        };

        let conversion: Conversion = format.parse().map_err(|e| format!("{}", e))?;

        let to_timestamp = |value| match value {
            Value::String(_) => conversion
                .convert(value.into())
                .map(Into::into)
                .map_err(|e| e.to_string().into()),
            Value::Timestamp(_) => Ok(value),
            _ => Err("unable to convert value to integer".into()),
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
    use chrono::{DateTime, Utc};

    #[test]
    fn parse_timestamp() {
        let cases = vec![
            (
                map![],
                Err("path error: missing path: foo".into()),
                ParseTimestampFn::new("%a %b %e %T %Y", Box::new(Path::from("foo")), None),
            ),
            (
                map![],
                Ok(Some(Value::Timestamp(
                    DateTime::parse_from_str(
                        "1983 Apr 13 12:09:14.274 +0000",
                        "%Y %b %d %H:%M:%S%.3f %z",
                    )
                    .unwrap()
                    .with_timezone(&Utc),
                ))),
                ParseTimestampFn::new(
                    "%Y %b %d %H:%M:%S%.3f %z",
                    Box::new(Path::from("foo")),
                    Some("1983 Apr 13 12:09:14.274 +0000".into()),
                ),
            ),
            (
                map![
                    "foo": DateTime::parse_from_rfc2822("Wed, 16 Oct 2019 12:00:00 +0000")
                            .unwrap()
                            .with_timezone(&Utc),
                ],
                Ok(Some(
                    DateTime::parse_from_rfc2822("Wed, 16 Oct 2019 12:00:00 +0000")
                        .unwrap()
                        .with_timezone(&Utc)
                        .into(),
                )),
                ParseTimestampFn::new("%d/%m/%Y:%H:%M:%S %z", Box::new(Path::from("foo")), None),
            ),
            (
                map!["foo": "16/10/2019:12:00:00 +0000"],
                Ok(Some(
                    DateTime::parse_from_rfc2822("Wed, 16 Oct 2019 12:00:00 +0000")
                        .unwrap()
                        .with_timezone(&Utc)
                        .into(),
                )),
                ParseTimestampFn::new("%d/%m/%Y:%H:%M:%S %z", Box::new(Path::from("foo")), None),
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
