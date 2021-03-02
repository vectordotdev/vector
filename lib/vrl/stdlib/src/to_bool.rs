use shared::conversion::Conversion;
use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct ToBool;

impl Function for ToBool {
    fn identifier(&self) -> &'static str {
        "to_bool"
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
                title: "integer (0)",
                source: "to_bool(0)",
                result: Ok("false"),
            },
            Example {
                title: "integer (other)",
                source: "to_bool(2)",
                result: Ok("true"),
            },
            Example {
                title: "float (0)",
                source: "to_bool(0.0)",
                result: Ok("false"),
            },
            Example {
                title: "float (other)",
                source: "to_bool(5.6)",
                result: Ok("true"),
            },
            Example {
                title: "true",
                source: "to_bool(true)",
                result: Ok("true"),
            },
            Example {
                title: "false",
                source: "to_bool(false)",
                result: Ok("false"),
            },
            Example {
                title: "null",
                source: "to_bool(null)",
                result: Ok("false"),
            },
            Example {
                title: "true string",
                source: "to_bool!(s'true')",
                result: Ok("true"),
            },
            Example {
                title: "yes string",
                source: "to_bool!(s'yes')",
                result: Ok("true"),
            },
            Example {
                title: "y string",
                source: "to_bool!(s'y')",
                result: Ok("true"),
            },
            Example {
                title: "non-zero integer string",
                source: "to_bool!(s'1')",
                result: Ok("true"),
            },
            Example {
                title: "false string",
                source: "to_bool!(s'false')",
                result: Ok("false"),
            },
            Example {
                title: "no string",
                source: "to_bool!(s'no')",
                result: Ok("false"),
            },
            Example {
                title: "n string",
                source: "to_bool!(s'n')",
                result: Ok("false"),
            },
            Example {
                title: "zero integer string",
                source: "to_bool!(s'0')",
                result: Ok("false"),
            },
            Example {
                title: "invalid string",
                source: "to_bool!(s'foobar')",
                result: Err(
                    r#"function call error for "to_bool" at (0:19): Invalid boolean value "foobar""#,
                ),
            },
            Example {
                title: "timestamp",
                source: "to_bool!(t'2020-01-01T00:00:00Z')",
                result: Err(
                    r#"function call error for "to_bool" at (0:33): unable to coerce "timestamp" into "boolean""#,
                ),
            },
            Example {
                title: "array",
                source: "to_bool!([])",
                result: Err(
                    r#"function call error for "to_bool" at (0:12): unable to coerce "array" into "boolean""#,
                ),
            },
            Example {
                title: "object",
                source: "to_bool!({})",
                result: Err(
                    r#"function call error for "to_bool" at (0:12): unable to coerce "object" into "boolean""#,
                ),
            },
            Example {
                title: "regex",
                source: "to_bool!(r'foo')",
                result: Err(
                    r#"function call error for "to_bool" at (0:16): unable to coerce "regex" into "boolean""#,
                ),
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");

        Ok(Box::new(ToBoolFn { value }))
    }
}

#[derive(Debug, Clone)]
struct ToBoolFn {
    value: Box<dyn Expression>,
}

impl Expression for ToBoolFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        use Value::*;

        let value = self.value.resolve(ctx)?;

        match value {
            Boolean(_) => Ok(value),
            Integer(v) => Ok(Boolean(v != 0)),
            Float(v) => Ok(Boolean(v != 0.0)),
            Null => Ok(Boolean(false)),
            Bytes(v) => Conversion::Boolean
                .convert(v)
                .map_err(|e| e.to_string().into()),
            v => Err(format!(r#"unable to coerce {} into "boolean""#, v.kind()).into()),
        }
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        TypeDef::new()
            .with_fallibility(
                self.value.type_def(state).has_kind(
                    Kind::Bytes | Kind::Timestamp | Kind::Array | Kind::Object | Kind::Regex,
                ),
            )
            .boolean()
    }
}

// #[cfg(test)]
// mod tests {
//     use super::*;

//     vrl::test_type_def![
//         boolean_infallible {
//             expr: |_| ToBoolFn { value: lit!(true).boxed() },
//             def: TypeDef { kind: Kind::Boolean, ..Default::default() },
//         }

//         integer_infallible {
//             expr: |_| ToBoolFn { value: lit!(1).boxed() },
//             def: TypeDef { kind: Kind::Boolean, ..Default::default() },
//         }

//         float_infallible {
//             expr: |_| ToBoolFn { value: lit!(1.0).boxed() },
//             def: TypeDef { kind: Kind::Boolean, ..Default::default() },
//         }

//         null_infallible {
//             expr: |_| ToBoolFn { value: lit!(null).boxed() },
//             def: TypeDef { kind: Kind::Boolean, ..Default::default() },
//         }

//         string_fallible {
//             expr: |_| ToBoolFn { value: lit!("foo").boxed() },
//             def: TypeDef { fallible: true, kind: Kind::Boolean, ..Default::default() },
//         }

//         map_fallible {
//             expr: |_| ToBoolFn { value: map!{}.boxed() },
//             def: TypeDef { fallible: true, kind: Kind::Boolean, ..Default::default() },
//         }

//         array_fallible {
//             expr: |_| ToBoolFn { value: array![].boxed() },
//             def: TypeDef { fallible: true, kind: Kind::Boolean, ..Default::default() },
//         }

//         timestamp_fallible {
//             expr: |_| ToBoolFn { value: Literal::from(chrono::Utc::now()).boxed() },
//             def: TypeDef { fallible: true, kind: Kind::Boolean, ..Default::default() },
//         }

//         fallible_value_without_default {
//             expr: |_| ToBoolFn { value: lit!("foo").boxed() },
//             def: TypeDef {
//                 fallible: true,
//                 kind: Kind::Boolean,
//                 ..Default::default()
//             },
//         }
//     ];

//     #[test]
//     fn to_bool() {
//         use crate::map;

//         let cases = vec![
//             (
//                 map!["foo": "true"],
//                 Ok(Value::Boolean(true)),
//                 ToBoolFn::new(Box::new(Path::from("foo"))),
//             ),
//             (
//                 map!["foo": 20],
//                 Ok(Value::Boolean(true)),
//                 ToBoolFn::new(Box::new(Path::from("foo"))),
//             ),
//         ];

//         let mut state = state::Program::default();

//         for (object, exp, func) in cases {
//             let mut object: Value = object.into();
//             let got = func
//                 .resolve(&mut ctx)
//                 .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));

//             assert_eq!(got, exp);
//         }
//     }
// }
