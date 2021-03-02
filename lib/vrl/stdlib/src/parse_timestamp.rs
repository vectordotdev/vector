use shared::{conversion::Conversion, TimeZone};
use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct ParseTimestamp;

impl Function for ParseTimestamp {
    fn identifier(&self) -> &'static str {
        "parse_timestamp"
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "valid",
            source: r#"parse_timestamp!("11-Feb-2021 16:00 +00:00", format: "%v %R %z")"#,
            result: Ok("t'2021-02-11T16:00:00Z'"),
        }]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");
        let format = arguments.required("format");

        Ok(Box::new(ParseTimestampFn { value, format }))
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::BYTES | kind::TIMESTAMP,
                required: true,
            },
            Parameter {
                keyword: "format",
                kind: kind::BYTES,
                required: true,
            },
        ]
    }
}

#[derive(Debug, Clone)]
struct ParseTimestampFn {
    value: Box<dyn Expression>,
    format: Box<dyn Expression>,
}

impl ParseTimestampFn {
    // #[cfg(test)]
    // fn new(format: &str, value: Box<dyn Expression>) -> Self {
    //     let format = Box::new(Literal::from(format));

    //     Self { value, format }
    // }
}

impl Expression for ParseTimestampFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        let format = self.format.resolve(ctx);

        match value {
            Value::Bytes(v) => Conversion::parse(
                format
                    .map(|v| format!("timestamp|{}", String::from_utf8_lossy(&v.unwrap_bytes())))?,
                TimeZone::Local,
            )
            .map_err(|e| format!("{}", e))?
            .convert(v)
            .map_err(|e| e.to_string().into()),
            Value::Timestamp(_) => Ok(value),
            _ => Err("unable to convert value to timestamp".into()),
        }
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new()
            .fallible() // Always fallible because the format needs to be parsed at runtime
            .timestamp()
    }
}

#[cfg(test)]
mod tests {
    /*
    use super::*;
    use chrono::{DateTime, Utc};
    use shared::btreemap;

    vrl::test_type_def![
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
                .resolve(&mut ctx)
                .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));

            assert_eq!(got, exp);
        }
    }
    */
}
