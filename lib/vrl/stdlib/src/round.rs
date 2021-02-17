use crate::util::round_to_precision;
use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Round;

impl Function for Round {
    fn identifier(&self) -> &'static str {
        "round"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::INTEGER | kind::FLOAT,
                required: true,
            },
            Parameter {
                keyword: "precision",
                kind: kind::INTEGER,
                required: false,
            },
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "round up",
                source: r#"round(5.5)"#,
                result: Ok("6.0"),
            },
            Example {
                title: "round down",
                source: r#"round(5.45)"#,
                result: Ok("5.0"),
            },
            Example {
                title: "precision",
                source: r#"round(5.45, 1)"#,
                result: Ok("5.5"),
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");
        let precision = arguments.optional("precision").unwrap_or(expr!(0));

        Ok(Box::new(RoundFn { value, precision }))
    }
}

#[derive(Debug, Clone)]
struct RoundFn {
    value: Box<dyn Expression>,
    precision: Box<dyn Expression>,
}

impl Expression for RoundFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let precision = self.precision.resolve(ctx)?.unwrap_integer();

        match self.value.resolve(ctx)? {
            Value::Float(f) => Ok(round_to_precision(f.into_inner(), precision, f64::round).into()),
            v @ Value::Integer(_) => Ok(v),
            _ => unreachable!(),
        }
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().infallible().integer()
    }
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use crate::map;

//     vrl::test_type_def![
//         value_float {
//             expr: |_| RoundFn {
//                 value: Literal::from(1.0).boxed(),
//                 precision: None,
//             },
//             def: TypeDef { kind: Kind::Float, ..Default::default() },
//         }

//         value_integer {
//             expr: |_| RoundFn {
//                 value: Literal::from(1).boxed(),
//                 precision: None,
//             },
//             def: TypeDef { kind: Kind::Integer, ..Default::default() },
//         }

//         value_float_or_integer {
//             expr: |_| RoundFn {
//                 value: Variable::new("foo".to_owned(), None).boxed(),
//                 precision: None,
//             },
//             def: TypeDef { fallible: true, kind: Kind::Integer | Kind::Float, ..Default::default() },
//         }

//         fallible_precision {
//             expr: |_| RoundFn {
//                 value: Literal::from(1).boxed(),
//                 precision: Some(Variable::new("foo".to_owned(), None).boxed()),
//             },
//             def: TypeDef { fallible: true, kind: Kind::Integer, ..Default::default() },
//         }
//     ];

//     #[test]
//     fn round() {
//         let cases = vec![
//             (
//                 map!["foo": 1234.2],
//                 Ok(1234.0.into()),
//                 RoundFn::new(Box::new(Path::from("foo")), None),
//             ),
//             (
//                 map![],
//                 Ok(1235.0.into()),
//                 RoundFn::new(Box::new(Literal::from(Value::Float(1234.8))), None),
//             ),
//             (
//                 map![],
//                 Ok(1234.into()),
//                 RoundFn::new(Box::new(Literal::from(Value::Integer(1234))), None),
//             ),
//             (
//                 map![],
//                 Ok(1234.4.into()),
//                 RoundFn::new(
//                     Box::new(Literal::from(Value::Float(1234.39429))),
//                     Some(Box::new(Literal::from(1))),
//                 ),
//             ),
//             (
//                 map![],
//                 Ok(1234.5679.into()),
//                 RoundFn::new(
//                     Box::new(Literal::from(Value::Float(1234.56789))),
//                     Some(Box::new(Literal::from(4))),
//                 ),
//             ),
//             (
//                 map![],
//                 Ok(9876543210123456789098765432101234567890987654321.98765.into()),
//                 RoundFn::new(
//                     Box::new(Literal::from(
//                         9876543210123456789098765432101234567890987654321.987654321,
//                     )),
//                     Some(Box::new(Literal::from(5))),
//                 ),
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
