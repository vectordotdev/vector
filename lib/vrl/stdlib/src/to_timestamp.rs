use chrono::{TimeZone as _, Utc};
use shared::{conversion::Conversion, TimeZone};
use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct ToTimestamp;

impl Function for ToTimestamp {
    fn identifier(&self) -> &'static str {
        "to_timestamp"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            kind: kind::ANY,
            required: true,
        }]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "timestamp",
                source: "to_timestamp(t'2020-01-01T00:00:00Z')",
                result: Ok("t'2020-01-01T00:00:00Z'"),
            },
            Example {
                title: "integer",
                source: "to_timestamp(5)",
                result: Ok("t'1970-01-01T00:00:05Z'"),
            },
            Example {
                title: "float",
                source: "to_timestamp(5.6)",
                result: Ok("t'1970-01-01T00:00:05.600Z'"),
            },
            Example {
                title: "string valid",
                source: "to_timestamp!(s'2020-01-01T00:00:00Z')",
                result: Ok("t'2020-01-01T00:00:00Z'"),
            },
            Example {
                title: "string invalid",
                source: "to_timestamp!(s'foo')",
                result: Err(
                    r#"function call error for "to_timestamp" at (0:21): No matching timestamp format found for "foo""#,
                ),
            },
            Example {
                title: "true",
                source: "to_timestamp!(true)",
                result: Err(
                    r#"function call error for "to_timestamp" at (0:19): unable to coerce "boolean" into "timestamp""#,
                ),
            },
            Example {
                title: "false",
                source: "to_timestamp!(false)",
                result: Err(
                    r#"function call error for "to_timestamp" at (0:20): unable to coerce "boolean" into "timestamp""#,
                ),
            },
            Example {
                title: "null",
                source: "to_timestamp!(null)",
                result: Err(
                    r#"function call error for "to_timestamp" at (0:19): unable to coerce "null" into "timestamp""#,
                ),
            },
            Example {
                title: "array",
                source: "to_timestamp!([])",
                result: Err(
                    r#"function call error for "to_timestamp" at (0:17): unable to coerce "array" into "timestamp""#,
                ),
            },
            Example {
                title: "object",
                source: "to_timestamp!({})",
                result: Err(
                    r#"function call error for "to_timestamp" at (0:17): unable to coerce "object" into "timestamp""#,
                ),
            },
            Example {
                title: "regex",
                source: "to_timestamp!(r'foo')",
                result: Err(
                    r#"function call error for "to_timestamp" at (0:21): unable to coerce "regex" into "timestamp""#,
                ),
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");

        Ok(Box::new(ToTimestampFn { value }))
    }
}

#[derive(Debug, Clone)]
struct ToTimestampFn {
    value: Box<dyn Expression>,
}

impl Expression for ToTimestampFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        use Value::*;

        let value = match self.value.resolve(ctx)? {
            v @ Timestamp(_) => v,
            Integer(v) => Utc.timestamp(v, 0).into(),
            Float(v) => Utc
                .timestamp(
                    v.trunc() as i64,
                    (v.fract() * 1_000_000_000.0).round() as u32,
                )
                .into(),
            Bytes(v) => Conversion::Timestamp(TimeZone::Local)
                .convert::<Value>(v)
                .map_err(|err| err.to_string())?,
            v => return Err(format!(r#"unable to coerce {} into "timestamp""#, v.kind()).into()),
        };

        Ok(value)
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.value
            .type_def(state)
            .fallible_unless(Kind::Timestamp | Kind::Integer | Kind::Float)
            .timestamp()
    }
}

// #[cfg(test)]
// mod tests {
//     use super::*;

//     vrl::test_type_def![
//         timestamp_infallible {
//             expr: |_| ToTimestampFn { value: Literal::from(chrono::Utc::now()).boxed() },
//             def: TypeDef { kind: Kind::Timestamp, ..Default::default() },
//         }

//         integer_infallible {
//             expr: |_| ToTimestampFn { value: lit!(1).boxed() },
//             def: TypeDef { kind: Kind::Timestamp, ..Default::default() },
//         }

//         float_infallible {
//             expr: |_| ToTimestampFn { value: lit!(1.0).boxed() },
//             def: TypeDef { kind: Kind::Timestamp, ..Default::default() },
//         }

//         null_fallible {
//             expr: |_| ToTimestampFn { value: lit!(null).boxed() },
//             def: TypeDef {
//                 fallible: true,
//                 kind: Kind::Timestamp,
//                 ..Default::default()
//             },
//         }

//         string_fallible {
//             expr: |_| ToTimestampFn { value: lit!("foo").boxed() },
//             def: TypeDef {
//                 fallible: true,
//                 kind: Kind::Timestamp,
//                 ..Default::default()
//             },
//         }

//         map_fallible {
//             expr: |_| ToTimestampFn { value: map!{}.boxed() },
//             def: TypeDef {
//                 fallible: true,
//                 kind: Kind::Timestamp,
//                 ..Default::default()
//             },
//         }

//         array_fallible {
//             expr: |_| ToTimestampFn { value: array![].boxed() },
//             def: TypeDef {
//                 fallible: true,
//                 kind: Kind::Timestamp,
//                 ..Default::default()
//             },
//         }

//         boolean_fallible {
//             expr: |_| ToTimestampFn { value: lit!(true).boxed() },
//             def: TypeDef {
//                 fallible: true,
//                 kind: Kind::Timestamp,
//                 ..Default::default()
//             },
//         }
//     ];
// }
