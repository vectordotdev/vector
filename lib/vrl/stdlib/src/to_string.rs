use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct ToString;

impl Function for ToString {
    fn identifier(&self) -> &'static str {
        "to_string"
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
                title: "string",
                source: "to_string(s'foo')",
                result: Ok("foo"),
            },
            Example {
                title: "integer",
                source: "to_string(5)",
                result: Ok("s'5'"),
            },
            Example {
                title: "float",
                source: "to_string(5.6)",
                result: Ok("s'5.6'"),
            },
            Example {
                title: "true",
                source: "to_string(true)",
                result: Ok("s'true'"),
            },
            Example {
                title: "false",
                source: "to_string(false)",
                result: Ok("s'false'"),
            },
            Example {
                title: "null",
                source: "to_string(null)",
                result: Ok(""),
            },
            Example {
                title: "timestamp",
                source: "to_string(t'2020-01-01T00:00:00Z')",
                result: Ok("2020-01-01T00:00:00Z"),
            },
            Example {
                title: "array",
                source: "to_string!([])",
                result: Err(
                    r#"function call error for "to_string" at (0:14): unable to coerce "array" into "string""#,
                ),
            },
            Example {
                title: "object",
                source: "to_string!({})",
                result: Err(
                    r#"function call error for "to_string" at (0:14): unable to coerce "object" into "string""#,
                ),
            },
            Example {
                title: "regex",
                source: "to_string!(r'foo')",
                result: Err(
                    r#"function call error for "to_string" at (0:18): unable to coerce "regex" into "string""#,
                ),
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");

        Ok(Box::new(ToStringFn { value }))
    }
}

#[derive(Debug, Clone)]
struct ToStringFn {
    value: Box<dyn Expression>,
}

impl Expression for ToStringFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        use chrono::SecondsFormat;
        use Value::*;

        let value = match self.value.resolve(ctx)? {
            v @ Bytes(_) => v,
            Integer(v) => v.to_string().into(),
            Float(v) => v.to_string().into(),
            Boolean(v) => v.to_string().into(),
            Timestamp(v) => v.to_rfc3339_opts(SecondsFormat::AutoSi, true).into(),
            Null => "".into(),
            v => return Err(format!(r#"unable to coerce {} into "string""#, v.kind()).into()),
        };

        Ok(value)
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.value
            .type_def(state)
            .fallible_unless(
                Kind::Bytes
                    | Kind::Integer
                    | Kind::Float
                    | Kind::Boolean
                    | Kind::Null
                    | Kind::Timestamp,
            )
            .bytes()
    }
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//
//     vrl::test_type_def![
//         boolean_infallible {
//             expr: |_| ToStringFn { value: lit!(true).boxed() },
//             def: TypeDef { kind: Kind::Bytes, ..Default::default() },
//         }

//         integer_infallible {
//             expr: |_| ToStringFn { value: lit!(1).boxed() },
//             def: TypeDef { kind: Kind::Bytes, ..Default::default() },
//         }

//         float_infallible {
//             expr: |_| ToStringFn { value: lit!(1.0).boxed() },
//             def: TypeDef { kind: Kind::Bytes, ..Default::default() },
//         }

//         null_infallible {
//             expr: |_| ToStringFn { value: lit!(null).boxed() },
//             def: TypeDef { kind: Kind::Bytes, ..Default::default() },
//         }

//         string_infallible {
//             expr: |_| ToStringFn { value: lit!("foo").boxed() },
//             def: TypeDef { kind: Kind::Bytes, ..Default::default() },
//         }

//         timestamp_infallible {
//             expr: |_| ToStringFn { value: Literal::from(chrono::Utc::now()).boxed() },
//             def: TypeDef { kind: Kind::Bytes, ..Default::default() },
//         }

//         map_fallible {
//             expr: |_| ToStringFn { value: map!{}.boxed() },
//             def: TypeDef { fallible: true, kind: Kind::Bytes, ..Default::default() },
//         }

//         array_fallible {
//             expr: |_| ToStringFn { value: array![].boxed() },
//             def: TypeDef { fallible: true, kind: Kind::Bytes, ..Default::default() },
//         }
//     ];

//     #[test]
//     fn to_string() {
//         use crate::map;

//         let cases = vec![
//             (
//                 map!["foo": 20],
//                 Ok(Value::from("20")),
//                 ToStringFn::new(Box::new(Path::from("foo"))),
//             ),
//             (
//                 map!["foo": 20.5],
//                 Ok(Value::from("20.5")),
//                 ToStringFn::new(Box::new(Path::from("foo"))),
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
