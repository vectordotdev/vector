use chrono::format::{strftime::StrftimeItems, Item};
use chrono::{DateTime, Utc};
use remap::prelude::*;

#[derive(Debug)]
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
                accepts: |v| matches!(v, Value::String(_)),
                required: true,
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required_expr("value")?;
        let format = arguments.required_expr("format")?;

        Ok(Box::new(FormatTimestampFn { value, format }))
    }
}

#[derive(Debug)]
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
    fn execute(&self, state: &mut State, object: &mut dyn Object) -> Result<Option<Value>> {
        let format = required!(state, object, self.format, Value::String(b) => String::from_utf8_lossy(&b).into_owned());
        let ts = required!(state, object, self.value, Value::Timestamp(ts) => ts);

        try_format(&ts, &format).map(Into::into).map(Some)
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
    use crate::map;
    use chrono::TimeZone;

    #[test]
    fn format_timestamp() {
        let cases = vec![
            (
                map![],
                Err("path error: missing path: foo".into()),
                FormatTimestampFn::new(Box::new(Path::from("foo")), "%s"),
            ),
            (
                map![],
                Err("function call error: invalid format".into()),
                FormatTimestampFn::new(
                    Box::new(Literal::from(Value::from(Utc.timestamp(10, 0)))),
                    "%Q INVALID",
                ),
            ),
            (
                map![],
                Ok(Some("10".into())),
                FormatTimestampFn::new(
                    Box::new(Literal::from(Value::from(Utc.timestamp(10, 0)))),
                    "%s",
                ),
            ),
            (
                map![],
                Ok(Some("1970-01-01T00:00:10+00:00".into())),
                FormatTimestampFn::new(
                    Box::new(Literal::from(Value::from(Utc.timestamp(10, 0)))),
                    "%+",
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
