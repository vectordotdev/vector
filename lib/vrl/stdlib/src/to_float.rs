use shared::conversion::Conversion;
use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct ToFloat;

impl Function for ToFloat {
    fn identifier(&self) -> &'static str {
        "to_float"
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
                title: "integer",
                source: "to_float(5)",
                result: Ok("5.0"),
            },
            Example {
                title: "float",
                source: "to_float(5.6)",
                result: Ok("5.6"),
            },
            Example {
                title: "true",
                source: "to_float(true)",
                result: Ok("1.0"),
            },
            Example {
                title: "false",
                source: "to_float(false)",
                result: Ok("0.0"),
            },
            Example {
                title: "null",
                source: "to_float(null)",
                result: Ok("0.0"),
            },
            Example {
                title: "valid string",
                source: "to_float!(s'5.6')",
                result: Ok("5.6"),
            },
            Example {
                title: "invalid string",
                source: "to_float!(s'foobar')",
                result: Err(
                    r#"function call error for "to_float" at (0:20): Invalid floating point number "foobar": invalid float literal"#,
                ),
            },
            Example {
                title: "timestamp",
                source: "to_float!(t'2020-01-01T00:00:00Z')",
                result: Err(
                    r#"function call error for "to_float" at (0:34): unable to coerce "timestamp" into "float""#,
                ),
            },
            Example {
                title: "array",
                source: "to_float!([])",
                result: Err(
                    r#"function call error for "to_float" at (0:13): unable to coerce "array" into "float""#,
                ),
            },
            Example {
                title: "object",
                source: "to_float!({})",
                result: Err(
                    r#"function call error for "to_float" at (0:13): unable to coerce "object" into "float""#,
                ),
            },
            Example {
                title: "regex",
                source: "to_float!(r'foo')",
                result: Err(
                    r#"function call error for "to_float" at (0:17): unable to coerce "regex" into "float""#,
                ),
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");

        Ok(Box::new(ToFloatFn { value }))
    }
}

#[derive(Debug, Clone)]
struct ToFloatFn {
    value: Box<dyn Expression>,
}

impl Expression for ToFloatFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        use Value::*;

        let value = self.value.resolve(ctx)?;

        match value {
            Float(_) => Ok(value),
            Integer(v) => Ok((v as f64).into()),
            Boolean(v) => Ok(NotNan::new(if v { 1.0 } else { 0.0 }).unwrap().into()),
            Null => Ok(0.0.into()),
            Bytes(v) => Conversion::Float
                .convert(v)
                .map_err(|e| e.to_string().into()),
            v => Err(format!(r#"unable to coerce {} into "float""#, v.kind()).into()),
        }
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        TypeDef::new()
            .with_fallibility(
                self.value.type_def(state).has_kind(
                    Kind::Bytes | Kind::Timestamp | Kind::Array | Kind::Object | Kind::Regex,
                ),
            )
            .float()
    }
}

// #[cfg(test)]
// mod tests {
//     use super::*;

//     vrl::test_type_def![
//         boolean_infallible {
//             expr: |_| ToFloatFn { value: lit!(true).boxed() },
//             def: TypeDef { kind: Kind::Float, ..Default::default() },
//         }

//         integer_infallible {
//             expr: |_| ToFloatFn { value: lit!(1).boxed() },
//             def: TypeDef { kind: Kind::Float, ..Default::default() },
//         }

//         float_infallible {
//             expr: |_| ToFloatFn { value: lit!(1.0).boxed() },
//             def: TypeDef { kind: Kind::Float, ..Default::default() },
//         }

//         null_infallible {
//             expr: |_| ToFloatFn { value: lit!(null).boxed() },
//             def: TypeDef { kind: Kind::Float, ..Default::default() },
//         }

//         string_fallible {
//             expr: |_| ToFloatFn { value: lit!("foo").boxed() },
//             def: TypeDef { fallible: true, kind: Kind::Float, ..Default::default() },
//         }

//         map_fallible {
//             expr: |_| ToFloatFn { value: map!{}.boxed() },
//             def: TypeDef { fallible: true, kind: Kind::Float, ..Default::default() },
//         }

//         array_fallible {
//             expr: |_| ToFloatFn { value: array![].boxed() },
//             def: TypeDef { fallible: true, kind: Kind::Float, ..Default::default() },
//         }

//         timestamp_infallible {
//             expr: |_| ToFloatFn { value: Literal::from(chrono::Utc::now()).boxed() },
//             def: TypeDef { fallible: true, kind: Kind::Float, ..Default::default() },
//         }
//     ];

//     #[test]
//     fn to_float() {
//         let cases = vec![
//             (Ok(Value::Float(20.5)), ToFloatFn::new(lit!(20.5).boxed())),
//             (
//                 Ok(Value::Float(20.0)),
//                 ToFloatFn::new(Literal::from(value!(20)).boxed()),
//             ),
//         ];

//         let mut state = state::Program::default();

//         for (exp, func) in cases {
//             let got = func
//                 .execute(&mut state, &mut value!({}))
//                 .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));

//             assert_eq!(got, exp);
//         }
//     }
// }
