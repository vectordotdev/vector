use super::prelude::*;
use chrono::format::{strftime::StrftimeItems, Item};
use chrono::{DateTime, Utc};

#[derive(Debug)]
pub(in crate::mapping) struct FormatTimestampFn {
    query: Box<dyn Function>,
    format: Box<dyn Function>,
}

impl FormatTimestampFn {
    #[cfg(test)]
    pub(in crate::mapping) fn new(query: Box<dyn Function>, format: &str) -> Self {
        let format = Box::new(Literal::from(Value::from(format)));

        Self { query, format }
    }
}

impl Function for FormatTimestampFn {
    fn execute(&self, ctx: &Event) -> Result<QueryValue> {
        let format = required_value!(ctx, self.format, Value::Bytes(b) => String::from_utf8_lossy(&b).into_owned());
        let ts = required_value!(ctx, self.query, Value::Timestamp(ts) => ts);

        try_format(&ts, &format).map(QueryValue::from_value)
    }

    fn parameters() -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                accepts: |v| matches!(v, QueryValue::Value(Value::Timestamp(_))),
                required: true,
            },
            Parameter {
                keyword: "format",
                accepts: |v| matches!(v, QueryValue::Value(Value::Bytes(_))),
                required: true,
            },
        ]
    }
}

impl TryFrom<ArgumentList> for FormatTimestampFn {
    type Error = String;

    fn try_from(mut arguments: ArgumentList) -> Result<Self> {
        let query = arguments.required("value")?;
        let format = arguments.required("format")?;

        Ok(Self { query, format })
    }
}

fn try_format(dt: &DateTime<Utc>, format: &str) -> Result<String> {
    let items = StrftimeItems::new(format)
        .map(|item| match item {
            Item::Error => Err("invalid format".to_owned()),
            _ => Ok(item),
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(dt.format_with_items(items.into_iter()).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapping::query::path::Path;
    use chrono::TimeZone;

    #[test]
    fn format_timestamp() {
        let cases = vec![
            (
                Event::from(""),
                Err("path .foo not found in event".to_string()),
                FormatTimestampFn::new(Box::new(Path::from(vec![vec!["foo"]])), "%s"),
            ),
            (
                Event::from(""),
                Err("invalid format".to_owned()),
                FormatTimestampFn::new(
                    Box::new(Literal::from(Value::from(Utc.timestamp(10, 0)))),
                    "%Q INVALID",
                ),
            ),
            (
                Event::from(""),
                Ok(Value::from("10")),
                FormatTimestampFn::new(
                    Box::new(Literal::from(Value::from(Utc.timestamp(10, 0)))),
                    "%s",
                ),
            ),
            (
                Event::from(""),
                Ok(Value::from("1970-01-01T00:00:10+00:00")),
                FormatTimestampFn::new(
                    Box::new(Literal::from(Value::from(Utc.timestamp(10, 0)))),
                    "%+",
                ),
            ),
        ];

        for (input_event, exp, query) in cases {
            assert_eq!(query.execute(&input_event), exp.map(QueryValue::Value));
        }
    }
}
