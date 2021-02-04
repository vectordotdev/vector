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
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required("value")?.boxed();
        let format = arguments.required("format")?.boxed();

        Ok(Box::new(ParseTimestampFn { value, format }))
    }
}

#[derive(Debug, Clone)]
struct ParseTimestampFn {
    value: Box<dyn Expression>,
    format: Box<dyn Expression>,
}

impl ParseTimestampFn {
    #[cfg(test)]
    fn new(format: &str, value: Box<dyn Expression>) -> Self {
        let format = Box::new(Literal::from(format));

        Self { value, format }
    }
}

impl Expression for ParseTimestampFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let value = self.value.execute(state, object)?;
        let format = self.format.execute(state, object);

        match value {
            Value::Bytes(v) => format
                .map(|v| format!("timestamp|{}", String::from_utf8_lossy(&v.unwrap_bytes())))?
                .parse::<Conversion>()
                .map_err(|e| format!("{}", e))?
                .convert(v)
                .map_err(|e| e.to_string().into()),
            Value::Timestamp(_) => Ok(value),
            _ => Err("unable to convert value to integer".into()),
        }
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        use value::Kind;

        self.value
            .type_def(state)
            // Always fallible because the format needs to be parsed at runtime
            .into_fallible(true)
            .with_constraint(Kind::Timestamp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};
    use shared::btreemap;

    remap::test_type_def![
        value_string_fallible {
            expr: |_| ParseTimestampFn {
                value: lit!("<timestamp>").boxed(),
                format: lit!("<format>").boxed(),
            },
            def: TypeDef {
                fallible: true,
                kind: value::Kind::Timestamp,
                ..Default::default()
            },
        }

        value_timestamp_fallible {
            expr: |_| ParseTimestampFn {
                value: Literal::from(chrono::Utc::now()).boxed(),
                format: lit!("<format>").boxed(),
            },
            def: TypeDef {
                fallible: true,
                kind: value::Kind::Timestamp,
                ..Default::default()
            },
        }

        non_string_ot_timestamp_fallible {
            expr: |_| ParseTimestampFn {
                value: lit!(127).boxed(),
                format: lit!("<format>").boxed(),
            },
            def: TypeDef {
                fallible: true,
                kind: value::Kind::Timestamp,
                ..Default::default()
            },
        }
    ];

    #[test]
    fn parse_timestamp() {
        let cases = vec![
            (
                btreemap! {
                    "foo" => DateTime::parse_from_rfc2822("Wed, 16 Oct 2019 12:00:00 +0000")
                              .unwrap()
                              .with_timezone(&Utc),
                },
                Ok(
                    DateTime::parse_from_rfc2822("Wed, 16 Oct 2019 12:00:00 +0000")
                        .unwrap()
                        .with_timezone(&Utc)
                        .into(),
                ),
                ParseTimestampFn::new("%d/%m/%Y:%H:%M:%S %z", Box::new(Path::from("foo"))),
            ),
            (
                btreemap! { "foo" => "16/10/2019:12:00:00 +0000" },
                Ok(
                    DateTime::parse_from_rfc2822("Wed, 16 Oct 2019 12:00:00 +0000")
                        .unwrap()
                        .with_timezone(&Utc)
                        .into(),
                ),
                ParseTimestampFn::new("%d/%m/%Y:%H:%M:%S %z", Box::new(Path::from("foo"))),
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
