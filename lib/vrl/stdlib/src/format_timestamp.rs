use chrono::format::{strftime::StrftimeItems, Item};
use chrono::{DateTime, Utc};
use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct FormatTimestamp;

impl Function for FormatTimestamp {
    fn identifier(&self) -> &'static str {
        "format_timestamp"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::TIMESTAMP,
                required: true,
            },
            Parameter {
                keyword: "format",
                kind: kind::BYTES,
                required: true,
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");
        let format = arguments.required("format");

        Ok(Box::new(FormatTimestampFn { value, format }))
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "format timestamp",
            source: r#"format_timestamp!(t'2021-02-10T23:32:00+00:00', "%d %B %Y %H:%M")"#,
            result: Ok("10 February 2021 23:32"),
        }]
    }
}

#[derive(Debug, Clone)]
struct FormatTimestampFn {
    value: Box<dyn Expression>,
    format: Box<dyn Expression>,
}

/*
impl FormatTimestampFn {
    #[cfg(test)]
    fn new(value: Box<dyn Expression>, format: &str) -> Self {
        let format = Box::new(Literal::from(Value::from(format)));

        Self { value, format }
    }
}
*/

impl Expression for FormatTimestampFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let bytes = self.format.resolve(ctx)?.try_bytes()?;
        let format = String::from_utf8_lossy(&bytes);
        let ts = self.value.resolve(ctx)?.try_timestamp()?;

        try_format(&ts, &format).map(Into::into)
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().fallible().bytes()
    }
}

fn try_format(dt: &DateTime<Utc>, format: &str) -> Result<String> {
    let items = StrftimeItems::new(format)
        .map(|item| match item {
            Item::Error => Err("invalid format".into()),
            _ => Ok(item),
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(dt.format_with_items(items.into_iter()).to_string())
}

/*
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    vrl::test_type_def![
        value_and_format {
            expr: |_| FormatTimestampFn {
                value: Literal::from(chrono::Utc::now()).boxed(),
                format: Literal::from("%s").boxed(),
            },
            def: TypeDef { fallible: true, kind: Kind::Bytes, ..Default::default() },
        }

        optional_value {
            expr: |_| FormatTimestampFn {
                value: Box::new(Noop),
                format: Literal::from("%s").boxed(),
            },
            def: TypeDef { fallible: true, kind: Kind::Bytes, ..Default::default() },
        }
    ];

    #[test]
    fn format_timestamp() {
        let cases = vec![
            (
                btreemap! {},
                Err("function call error: invalid format".into()),
                FormatTimestampFn::new(
                    Box::new(Literal::from(Value::from(Utc.timestamp(10, 0)))),
                    "%Q INVALID",
                ),
            ),
            (
                btreemap! {},
                Ok("10".into()),
                FormatTimestampFn::new(
                    Box::new(Literal::from(Value::from(Utc.timestamp(10, 0)))),
                    "%s",
                ),
            ),
            (
                btreemap! {},
                Ok("1970-01-01T00:00:10+00:00".into()),
                FormatTimestampFn::new(
                    Box::new(Literal::from(Value::from(Utc.timestamp(10, 0)))),
                    "%+",
                ),
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
}
*/
