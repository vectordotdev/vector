use std::str::FromStr;

use vrl::prelude::*;

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
        _state: &state::Compiler,
        _ctx: &FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");

        let unit = arguments
            .optional_enum("unit", Unit::all_value().as_slice())?
            .map(|s| {
                Unit::from_str(&s.try_bytes_utf8_lossy().expect("unit not bytes"))
                    .expect("validated enum")
            })
            .unwrap_or_default();

        Ok(Box::new(ToUnixTimestampFn { value, unit }))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Unit {
    Seconds,
    Milliseconds,
    Nanoseconds,
}

impl Unit {
    fn all_value() -> Vec<Value> {
        use Unit::*;

        vec![Seconds, Milliseconds, Nanoseconds]
            .into_iter()
            .map(|u| u.as_str().into())
            .collect::<Vec<_>>()
    }

    const fn as_str(self) -> &'static str {
        use Unit::*;

        match self {
            Seconds => "seconds",
            Milliseconds => "milliseconds",
            Nanoseconds => "nanoseconds",
        }
    }
}

impl Default for Unit {
    fn default() -> Self {
        Unit::Seconds
    }
}

impl FromStr for Unit {
    type Err = &'static str;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        use Unit::*;

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

impl Expression for ToUnixTimestampFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let ts = self.value.resolve(ctx)?.try_timestamp()?;

        let time = match self.unit {
            Unit::Seconds => ts.timestamp(),
            Unit::Milliseconds => ts.timestamp_millis(),
            Unit::Nanoseconds => ts.timestamp_nanos(),
        };

        Ok(time.into())
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().infallible().integer()
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
            want: Ok(1609459200i64),
            tdef: TypeDef::new().infallible().integer(),
        }

        milliseconds {
            args: func_args![value: chrono::Utc.ymd(2021, 1, 1).and_hms_milli(0, 0, 0, 0),
                             unit: "milliseconds"
            ],
            want: Ok(1609459200000i64),
            tdef: TypeDef::new().infallible().integer(),
        }

        nanoseconds {
             args: func_args![value: chrono::Utc.ymd(2021, 1, 1).and_hms_milli(0, 0, 0, 0),
                              unit: "nanoseconds"
             ],
             want: Ok(1609459200000000000i64),
             tdef: TypeDef::new().infallible().integer(),
         }
    ];
}
