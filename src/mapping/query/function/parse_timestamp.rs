use super::prelude::*;

#[derive(Debug)]
pub(in crate::mapping) struct ParseTimestampFn {
    query: Box<dyn Function>,
    format: Box<dyn Function>,
    default: Option<Box<dyn Function>>,
}

impl ParseTimestampFn {
    #[cfg(test)]
    pub(in crate::mapping) fn new(
        format: &str,
        query: Box<dyn Function>,
        default: Option<Value>,
    ) -> Self {
        let format = Box::new(Literal::from(Value::from(format)));
        let default = default.map(|v| Box::new(Literal::from(v)) as _);

        Self {
            query,
            format,
            default,
        }
    }
}

impl Function for ParseTimestampFn {
    fn execute(&self, ctx: &Event) -> Result<QueryValue> {
        let format = match self.format.execute(ctx)? {
            QueryValue::Value(Value::Bytes(b)) => {
                format!("timestamp|{}", String::from_utf8_lossy(&b))
            }
            v => unexpected_type!(v),
        };

        let conversion: Conversion = format.parse().map_err(|error| format!("{}", error))?;

        let result = match self.query.execute(ctx) {
            Ok(value) => match value {
                QueryValue::Value(value @ Value::Bytes(_)) => conversion
                    .convert(value)
                    .map(Into::into)
                    .map_err(|e| e.to_string()),
                QueryValue::Value(Value::Timestamp(_)) => Ok(value),
                _ => unexpected_type!(value),
            },
            Err(err) => Err(err),
        };

        if result.is_err() {
            if let Some(v) = &self.default {
                return match v.execute(ctx)? {
                    QueryValue::Value(Value::Bytes(v)) => conversion
                        .convert(Value::Bytes(v))
                        .map(Into::into)
                        .map_err(|e| e.to_string()),
                    QueryValue::Value(Value::Timestamp(v)) => Ok(Value::Timestamp(v).into()),
                    v => unexpected_type!(v),
                };
            }
        }
        result
    }

    fn parameters() -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                accepts: |v| matches!(v, QueryValue::Value(Value::Bytes(_)) | QueryValue::Value(Value::Timestamp(_))),
                required: true,
            },
            Parameter {
                keyword: "format",
                accepts: |v| matches!(v, QueryValue::Value(Value::Bytes(_))),
                required: true,
            },
            Parameter {
                keyword: "default",
                accepts: |v| matches!(v, QueryValue::Value(Value::Bytes(_)) | QueryValue::Value(Value::Timestamp(_))),
                required: false,
            },
        ]
    }
}

impl TryFrom<ArgumentList> for ParseTimestampFn {
    type Error = String;

    fn try_from(mut arguments: ArgumentList) -> Result<Self> {
        let query = arguments.required("value")?;
        let format = arguments.required("format")?;
        let default = arguments.optional("default");

        Ok(Self {
            query,
            format,
            default,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapping::query::path::Path;
    use chrono::{DateTime, Utc};

    #[test]
    fn parse_timestamp() {
        let cases = vec![
            (
                Event::from(""),
                Err("path .foo not found in event".to_string()),
                ParseTimestampFn::new(
                    "%a %b %e %T %Y",
                    Box::new(Path::from(vec![vec!["foo"]])),
                    None,
                ),
            ),
            (
                Event::from(""),
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
                    Box::new(Path::from(vec![vec!["foo"]])),
                    Some(Value::from("1983 Apr 13 12:09:14.274 +0000")),
                ),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert(
                        "foo",
                        Value::Timestamp(
                            DateTime::parse_from_rfc2822("Wed, 16 Oct 2019 12:00:00 +0000")
                                .unwrap()
                                .with_timezone(&Utc),
                        ),
                    );
                    event
                },
                Ok(Value::Timestamp(
                    DateTime::parse_from_rfc2822("Wed, 16 Oct 2019 12:00:00 +0000")
                        .unwrap()
                        .with_timezone(&Utc),
                )),
                ParseTimestampFn::new(
                    "%d/%m/%Y:%H:%M:%S %z",
                    Box::new(Path::from(vec![vec!["foo"]])),
                    None,
                ),
            ),
            (
                {
                    let mut event = Event::from("");
                    event
                        .as_mut_log()
                        .insert("foo", Value::from("16/10/2019:12:00:00 +0000"));
                    event
                },
                Ok(Value::Timestamp(
                    DateTime::parse_from_rfc2822("Wed, 16 Oct 2019 12:00:00 +0000")
                        .unwrap()
                        .with_timezone(&Utc),
                )),
                ParseTimestampFn::new(
                    "%d/%m/%Y:%H:%M:%S %z",
                    Box::new(Path::from(vec![vec!["foo"]])),
                    None,
                ),
            ),
        ];

        for (input_event, exp, query) in cases {
            assert_eq!(query.execute(&input_event), exp.map(QueryValue::Value));
        }
    }
}
