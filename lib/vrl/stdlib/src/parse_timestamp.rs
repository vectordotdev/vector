use ::value::Value;
use vector_common::conversion::Conversion;
use vrl::prelude::*;

fn parse_timestamp(value: Value, format: Value, ctx: &Context) -> Resolved {
    match value {
        Value::Bytes(v) => {
            let format = format.try_bytes_utf8_lossy()?;
            Conversion::parse(format!("timestamp|{format}"), *ctx.timezone())
                .map_err(|e| e.to_string())?
                .convert(v)
                .map_err(|e| e.to_string().into())
        }
        Value::Timestamp(_) => Ok(value),
        _ => Err("unable to convert value to timestamp".into()),
    }
}

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

    fn compile(
        &self,
        _state: &state::TypeState,
        _ctx: &mut FunctionCompileContext,
        arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");
        let format = arguments.required("format");

        Ok(ParseTimestampFn { value, format }.as_expr())
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

impl FunctionExpression for ParseTimestampFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        let format = self.format.resolve(ctx)?;
        parse_timestamp(value, format, ctx)
    }

    fn type_def(&self, _: &state::TypeState) -> TypeDef {
        TypeDef::timestamp().fallible(/* always fallible because the format needs to be parsed at runtime */)
    }
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Utc};

    use super::*;

    test_function![
        parse_timestamp => ParseTimestamp;

        parse_timestamp {
            args: func_args![
                value: DateTime::parse_from_rfc2822("Wed, 16 Oct 2019 12:00:00 +0000")
                    .unwrap()
                    .with_timezone(&Utc),
                format:"%d/%m/%Y:%H:%M:%S %z"
            ],
            want: Ok(value!(
                DateTime::parse_from_rfc2822("Wed, 16 Oct 2019 12:00:00 +0000")
                    .unwrap()
                    .with_timezone(&Utc)
            )),
            tdef: TypeDef::timestamp().fallible(),
            tz: vector_common::TimeZone::default(),
        }

        parse_text {
            args: func_args![
                value: "16/10/2019:12:00:00 +0000",
                format: "%d/%m/%Y:%H:%M:%S %z"
            ],
            want: Ok(value!(
                DateTime::parse_from_rfc2822("Wed, 16 Oct 2019 12:00:00 +0000")
                    .unwrap()
                    .with_timezone(&Utc)
            )),
            tdef: TypeDef::timestamp().fallible(),
            tz: vector_common::TimeZone::default(),
        }

        parse_text_with_tz {
            args: func_args![
                value: "16/10/2019:12:00:00",
                format:"%d/%m/%Y:%H:%M:%S"
            ],
            want: Ok(value!(
                DateTime::parse_from_rfc2822("Wed, 16 Oct 2019 10:00:00 +0000")
                    .unwrap()
                    .with_timezone(&Utc)
            )),
            tdef: TypeDef::timestamp().fallible(),
            tz: vector_common::TimeZone::Named(chrono_tz::Europe::Paris),
        }
    ];
}
