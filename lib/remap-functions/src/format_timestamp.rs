use chrono::format::{strftime::StrftimeItems, Item};
use chrono::{DateTime, Utc};
use remap::prelude::*;

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
                accepts: |v| matches!(v, Value::Timestamp(_)),
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

        Ok(Box::new(FormatTimestampFn { value, format }))
    }
}

#[derive(Debug, Clone)]
struct FormatTimestampFn {
    value: Box<dyn Expression>,
    format: Box<dyn Expression>,
}

impl FormatTimestampFn {
    #[cfg(test)]
    fn new(value: Box<dyn Expression>, format: &str) -> Self {
        let format = Box::new(Literal::from(Value::from(format)));

        Self { value, format }
    }
}

impl Expression for FormatTimestampFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let bytes = self.format.execute(state, object)?.try_bytes()?;
        let format = String::from_utf8_lossy(&bytes);
        let ts = self.value.execute(state, object)?.try_timestamp()?;

        try_format(&ts, &format).map(Into::into)
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        let format_def = self
            .format
            .type_def(state)
            .fallible_unless(value::Kind::Bytes);

        self.value
            .type_def(state)
            .fallible_unless(value::Kind::Timestamp)
            .merge(format_def)
            .into_fallible(true) // due to `try_format`
            .with_constraint(value::Kind::Bytes)
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use shared::btreemap;
    use value::Kind;

    remap::test_type_def![
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
                .execute(&mut state, &mut object)
                .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));

            assert_eq!(got, exp);
        }
    }
}
