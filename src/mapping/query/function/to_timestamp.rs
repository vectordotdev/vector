use super::prelude::*;
use chrono::{TimeZone, Utc};

#[derive(Debug)]
pub(in crate::mapping) struct ToTimestampFn {
    query: Box<dyn Function>,
    default: Option<Box<dyn Function>>,
}

impl ToTimestampFn {
    #[cfg(test)]
    pub(in crate::mapping) fn new(query: Box<dyn Function>, default: Option<Value>) -> Self {
        let default = default.map(|v| Box::new(Literal::from(v)) as _);
        Self { query, default }
    }
}

impl Function for ToTimestampFn {
    fn execute(&self, ctx: &Event) -> Result<Value> {
        self.query
            .execute(ctx)
            .and_then(to_timestamp)
            .or_else(|err| {
                self.default
                    .as_ref()
                    .ok_or(err)
                    .and_then(|v| v.execute(ctx))
                    .and_then(to_timestamp)
            })
    }

    fn parameters() -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                accepts: |v| matches!(v, Value::Integer(_) | Value::Bytes(_) | Value::Timestamp(_)),
                required: true,
            },
            Parameter {
                keyword: "default",
                accepts: |v| matches!(v, Value::Integer(_) | Value::Bytes(_) | Value::Timestamp(_)),
                required: false,
            },
        ]
    }
}

impl TryFrom<ArgumentList> for ToTimestampFn {
    type Error = String;

    fn try_from(mut arguments: ArgumentList) -> Result<Self> {
        let query = arguments.required("value")?;
        let default = arguments.optional("default");

        Ok(Self { query, default })
    }
}

fn to_timestamp(value: Value) -> Result<Value> {
    match value {
        Value::Bytes(_) => Conversion::Timestamp
            .convert(value)
            .map_err(|e| e.to_string()),
        Value::Integer(i) => Ok(Value::Timestamp(Utc.timestamp(i, 0))),
        Value::Timestamp(_) => Ok(value),
        _ => Err("unable to parse non-string or integer type to timestamp".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapping::query::path::Path;
    use chrono::DateTime;

    #[test]
    fn to_timestamp() {
        let cases = vec![
            (
                Event::from(""),
                Err("path .foo not found in event".to_string()),
                ToTimestampFn::new(Box::new(Path::from(vec![vec!["foo"]])), None),
            ),
            (
                Event::from(""),
                Ok(Value::Timestamp(
                    DateTime::parse_from_rfc3339("1970-01-01T00:00:10Z")
                        .unwrap()
                        .with_timezone(&Utc),
                )),
                ToTimestampFn::new(
                    Box::new(Path::from(vec![vec!["foo"]])),
                    Some(Value::Integer(10)),
                ),
            ),
            (
                Event::from(""),
                Ok(Value::Timestamp(
                    DateTime::parse_from_rfc3339("1970-01-01T00:00:10Z")
                        .unwrap()
                        .with_timezone(&Utc),
                )),
                ToTimestampFn::new(
                    Box::new(Path::from(vec![vec!["foo"]])),
                    Some(Value::from("1970-01-01T00:00:10Z")),
                ),
            ),
            (
                Event::from(""),
                Ok(Value::Timestamp(
                    DateTime::parse_from_rfc3339("1970-01-01T00:00:10Z")
                        .unwrap()
                        .with_timezone(&Utc),
                )),
                ToTimestampFn::new(
                    Box::new(Path::from(vec![vec!["foo"]])),
                    Some(Value::Bytes("10".into())),
                ),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert(
                        "foo",
                        Value::Timestamp(
                            DateTime::parse_from_rfc3339("1970-02-01T00:00:10Z")
                                .unwrap()
                                .with_timezone(&Utc),
                        ),
                    );
                    event
                },
                Ok(Value::Timestamp(
                    DateTime::parse_from_rfc3339("1970-02-01T00:00:10Z")
                        .unwrap()
                        .with_timezone(&Utc),
                )),
                ToTimestampFn::new(Box::new(Path::from(vec![vec!["foo"]])), None),
            ),
        ];

        for (input_event, exp, query) in cases {
            assert_eq!(query.execute(&input_event), exp);
        }
    }
}
