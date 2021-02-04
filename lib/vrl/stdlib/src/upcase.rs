use vrl::prelude::*;
use std::convert::TryFrom;

#[derive(Clone, Copy, Debug)]
pub struct Upcase;

impl Function for Upcase {
    fn identifier(&self) -> &'static str {
        "upcase"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            kind: kind::BYTES,
            required: true,
        }]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value")?;

        Ok(Box::new(UpcaseFn { value }))
    }
}

#[derive(Debug)]
struct UpcaseFn {
    value: Box<dyn Expression>,
}

impl Expression for UpcaseFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let bytes = self.value.resolve(ctx)?.unwrap_bytes();

        Ok(String::from_utf8_lossy(&bytes).to_uppercase().into())
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().bytes().infallible()
    }
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use crate::map;
//     use value::Kind;

//     vrl::test_type_def![
//         string {
//             expr: |_| UpcaseFn { value: Literal::from("foo").boxed() },
//             def: TypeDef { kind: Kind::Bytes, ..Default::default() },
//         }

//         non_string {
//             expr: |_| UpcaseFn { value: Literal::from(true).boxed() },
//             def: TypeDef {
//                 fallible: true,
//                 kind: Kind::Bytes,
//                 ..Default::default()
//             },
//         }
//     ];

//     #[test]
//     fn upcase() {
//         let cases = vec![(
//             map!["foo": "foo 2 bar"],
//             Ok(Value::from("FOO 2 BAR")),
//             UpcaseFn::new(Box::new(Path::from("foo"))),
//         )];

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
