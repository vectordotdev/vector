use remap::prelude::*;
use shared::conversion::Conversion;

#[derive(Clone, Copy, Debug)]
pub struct ParseTimestamp;

impl Function for ParseTimestamp {
    fn identifier(&self) -> &'static str {
        "parse_timestamp"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                accepts: |v| matches!(v, Value::Bytes(_) | Value::Timestamp(_)),
                required: true,
            },
            Parameter {
                keyword: "format",
                accepts: |v| matches!(v, Value::Bytes(_)),
                required: true,
            },
            Parameter {
                keyword: "default",
                accepts: |v| matches!(v, Value::Bytes(_) | Value::Timestamp(_)),
                required: false,
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required("value")?.boxed();
        let format = arguments.required("format")?.boxed();
        let default = arguments.optional("default").map(Expr::boxed);

        Ok(Box::new(ParseTimestampFn {
            value,
            format,
            default,
        }))
    }
}

#[derive(Debug, Clone)]
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
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let format = self.format.execute(state, object);

        let to_timestamp = |value| match value {
            Value::Bytes(v) => format
                .clone()
                .map(|v| format!("timestamp|{}", String::from_utf8_lossy(&v.unwrap_bytes())))?
                .parse::<Conversion>()
                .map_err(|e| format!("{}", e))?
                .convert(v)
                .map_err(|e| e.to_string().into()),
            Value::Timestamp(_) => Ok(value),
            _ => Err("unable to convert value to integer".into()),
        };

        crate::util::convert_value_or_default(
            self.value.execute(state, object),
            self.default.as_ref().map(|v| v.execute(state, object)),
            to_timestamp,
        )
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        let value_def = self
            .value
            .type_def(state)
            .fallible_unless(value::Kind::Timestamp);

        let default_def = self
            .default
            .as_ref()
            .map(|v| v.type_def(state).fallible_unless(value::Kind::Timestamp));

        // The `format` type definition is only relevant if:
        //
        // 1. `value` can resolve to a string, AND:
        //   1. `default` is not defined, OR
        //   2. `default` can also resolve to a string.
        //
        // The `format` type is _always_ fallible, because its string has to be
        // parsed into a valid timestamp format.
        let format_def = if value_def.kind.contains(value::Kind::Bytes) {
            match &default_def {
                Some(def) if def.kind.contains(value::Kind::Bytes) => {
                    Some(self.format.type_def(state).into_fallible(true))
                }
                Some(_) => None,
                None => Some(self.format.type_def(state).into_fallible(true)),
            }
        } else {
            None
        };

        value_def
            .merge_with_default_optional(default_def)
            .merge_optional(format_def)
            .with_constraint(value::Kind::Timestamp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map;
    use chrono::{DateTime, Utc};

    remap::test_type_def![
        value_fallible_no_default {
            expr: |_| ParseTimestampFn {
                value: Literal::from("<timestamp>").boxed(),
                format: Literal::from("<format>").boxed(),
                default: None,
            },
            def: TypeDef {
                fallible: true,
                kind: value::Kind::Timestamp,
                ..Default::default()
            },
        }

        value_fallible_default_fallible {
            expr: |_| ParseTimestampFn {
                value: Literal::from("<timestamp>").boxed(),
                format: Literal::from("<format>").boxed(),
                default: Some(Literal::from("<timestamp>").boxed()),
            },
            def: TypeDef {
                fallible: true,
                kind: value::Kind::Timestamp,
                ..Default::default()
            },
        }

        value_fallible_default_infallible {
            expr: |_| ParseTimestampFn {
                value: Literal::from("<timestamp>").boxed(),
                format: Literal::from("<format>").boxed(),
                default: Some(Literal::from(chrono::Utc::now()).boxed()),
            },
            def: TypeDef {
                kind: value::Kind::Timestamp,
                ..Default::default()
            },
        }

        value_infallible_no_default {
            expr: |_| ParseTimestampFn {
                value: Literal::from(chrono::Utc::now()).boxed(),
                format: Literal::from("<format>").boxed(),
                default: None,
            },
            def: TypeDef {
                kind: value::Kind::Timestamp,
                ..Default::default()
            },
        }

        value_infallible_default_fallible {
            expr: |_| ParseTimestampFn {
                value: Literal::from(chrono::Utc::now()).boxed(),
                format: Literal::from("<format>").boxed(),
                default: Some(Literal::from("<timestamp>").boxed()),
            },
            def: TypeDef {
                kind: value::Kind::Timestamp,
                ..Default::default()
            },
        }

        value_infallible_default_infallible {
            expr: |_| ParseTimestampFn {
                value: Literal::from(chrono::Utc::now()).boxed(),
                format: Literal::from("<format>").boxed(),
                default: Some(Literal::from(chrono::Utc::now()).boxed()),
            },
            def: TypeDef {
                kind: value::Kind::Timestamp,
                ..Default::default()
            },
        }
    ];

    #[test]
    fn parse_timestamp() {
        let cases = vec![
            (
                map![],
                Ok(Value::Timestamp(
                    DateTime::parse_from_str(
                        "1983 Apr 13 12:09:14.274 +0000",
                        "%Y %b %d %H:%M:%S%.3f %z",
                    )
                    .unwrap()
                    .with_timezone(&Utc),
                )),
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
                Ok(
                    DateTime::parse_from_rfc2822("Wed, 16 Oct 2019 12:00:00 +0000")
                        .unwrap()
                        .with_timezone(&Utc)
                        .into(),
                ),
                ParseTimestampFn::new("%d/%m/%Y:%H:%M:%S %z", Box::new(Path::from("foo")), None),
            ),
            (
                map!["foo": "16/10/2019:12:00:00 +0000"],
                Ok(
                    DateTime::parse_from_rfc2822("Wed, 16 Oct 2019 12:00:00 +0000")
                        .unwrap()
                        .with_timezone(&Utc)
                        .into(),
                ),
                ParseTimestampFn::new("%d/%m/%Y:%H:%M:%S %z", Box::new(Path::from("foo")), None),
            ),
        ];

        let mut state = state::Program::default();

        for (object, exp, func) in cases {
            let mut object: Value = object.into();
            let got = func
                .execute(&mut state, &mut object)
                .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));

            assert_eq!(got, exp);
        }
    }
}
