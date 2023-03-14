use std::str::FromStr;

use ::value::Value;
use vrl::prelude::*;

fn to_unix_timestamp(value: Value, unit: Unit) -> Resolved {
    let ts = value.try_timestamp()?;
    let time = match unit {
        Unit::Seconds => ts.timestamp(),
        Unit::Milliseconds => ts.timestamp_millis(),
        Unit::Nanoseconds => ts.timestamp_nanos(),
    };
    Ok(time.into())
}

#[derive(Clone, Copy, Debug)]
pub struct ToUnixTimestamp;

impl Function for ToUnixTimestamp {
    fn identifier(&self) -> &'static str {
        "to_unix_timestamp"
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "default (seconds)",
                source: "to_unix_timestamp(t'2000-01-01T00:00:00Z')",
                result: Ok("946684800"),
            },
            Example {
                title: "milliseconds",
                source: r#"to_unix_timestamp(t'2010-01-01T00:00:00Z', unit: "milliseconds")"#,
                result: Ok("1262304000000"),
            },
            Example {
                title: "nanoseconds",
                source: r#"to_unix_timestamp(t'2020-01-01T00:00:00Z', unit: "nanoseconds")"#,
                result: Ok("1577836800000000000"),
            },
        ]
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::TIMESTAMP,
                required: true,
            },
            Parameter {
                keyword: "unit",
                kind: kind::BYTES,
                required: false,
            },
        ]
    }

    fn compile(
        &self,
        _state: &state::TypeState,
        _ctx: &mut FunctionCompileContext,
        arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");

        let unit = arguments
            .optional_enum("unit", Unit::all_value().as_slice())?
            .map(|s| {
                Unit::from_str(&s.try_bytes_utf8_lossy().expect("unit not bytes"))
                    .expect("validated enum")
            })
            .unwrap_or_default();

        Ok(ToUnixTimestampFn { value, unit }.as_expr())
    }
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
enum Unit {
    #[default]
    Seconds,
    Milliseconds,
    Nanoseconds,
}

impl Unit {
    fn all_value() -> Vec<Value> {
        use Unit::{Milliseconds, Nanoseconds, Seconds};

        vec![Seconds, Milliseconds, Nanoseconds]
            .into_iter()
            .map(|u| u.as_str().into())
            .collect::<Vec<_>>()
    }

    const fn as_str(self) -> &'static str {
        use Unit::{Milliseconds, Nanoseconds, Seconds};

        match self {
            Seconds => "seconds",
            Milliseconds => "milliseconds",
            Nanoseconds => "nanoseconds",
        }
    }
}

impl FromStr for Unit {
    type Err = &'static str;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        use Unit::{Milliseconds, Nanoseconds, Seconds};

        match s {
            "seconds" => Ok(Seconds),
            "milliseconds" => Ok(Milliseconds),
            "nanoseconds" => Ok(Nanoseconds),
            _ => Err("unit not recognized"),
        }
    }
}

#[derive(Debug, Clone)]
struct ToUnixTimestampFn {
    value: Box<dyn Expression>,
    unit: Unit,
}

impl FunctionExpression for ToUnixTimestampFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        let unit = self.unit;

        to_unix_timestamp(value, unit)
    }

    fn type_def(&self, _: &state::TypeState) -> TypeDef {
        TypeDef::integer().infallible()
    }
}

#[cfg(test)]
mod test {
    use chrono::TimeZone;

    use super::*;

    test_function![
        to_unix_timestamp => ToUnixTimestamp;

        seconds {
            args: func_args![value: chrono::Utc.ymd(2021, 1, 1).and_hms_milli(0, 0, 0, 0),
                             unit: "seconds"
            ],
            want: Ok(1_609_459_200_i64),
            tdef: TypeDef::integer().infallible(),
        }

        milliseconds {
            args: func_args![value: chrono::Utc.ymd(2021, 1, 1).and_hms_milli(0, 0, 0, 0),
                             unit: "milliseconds"
            ],
            want: Ok(1_609_459_200_000_i64),
            tdef: TypeDef::integer().infallible(),
        }

        nanoseconds {
             args: func_args![value: chrono::Utc.ymd(2021, 1, 1).and_hms_milli(0, 0, 0, 0),
                              unit: "nanoseconds"
             ],
             want: Ok(1_609_459_200_000_000_000_i64),
             tdef: TypeDef::integer().infallible(),
         }
    ];
}
