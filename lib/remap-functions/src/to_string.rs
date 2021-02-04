use remap::prelude::*;

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

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value")?;

        Ok(Box::new(ToStringFn { value }))
    }
}

#[derive(Debug)]
struct ToStringFn {
    value: Box<dyn Expression>,
}

impl Expression for ToStringFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        use Value::*;

        let value = match self.value.resolve(ctx)? {
            v @ Bytes(_) => v,
            Integer(v) => v.to_string().into(),
            Float(v) => v.to_string().into(),
            Boolean(v) => v.to_string().into(),
            Timestamp(v) => v.to_string().into(),
            Null => "".into(),
            Object(_) => Err("unable to coerce object into string")?,
            Array(_) => Err("unable to coerce array into string")?,
            Regex(_) => Err("unable to coerce regex into string")?,
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
//     use value::Kind;

//     remap::test_type_def![
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
//                 .execute(&mut state, &mut object)
//                 .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));

//             assert_eq!(got, exp);
//         }
//     }
// }
