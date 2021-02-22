use ::sha1::Digest;
use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Sha1;

impl Function for Sha1 {
    fn identifier(&self) -> &'static str {
        "sha1"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            kind: kind::BYTES,
            required: true,
        }]
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "sha1",
            source: r#"sha1("foobar")"#,
            result: Ok("8843d7f92416211de9ebb963ff4ce28125932878"),
        }]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");

        Ok(Box::new(Sha1Fn { value }))
    }
}

#[derive(Debug, Clone)]
struct Sha1Fn {
    value: Box<dyn Expression>,
}

impl Expression for Sha1Fn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?.unwrap_bytes();

        Ok(hex::encode(sha1::Sha1::digest(&value)).into())
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().infallible().bytes()
    }
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use crate::map;
//
//     vrl::test_type_def![
//         value_string {
//             expr: |_| Sha1Fn { value: Literal::from("foo").boxed() },
//             def: TypeDef { kind: Kind::Bytes, ..Default::default() },
//         }

//         value_non_string {
//             expr: |_| Sha1Fn { value: Literal::from(1).boxed() },
//             def: TypeDef { fallible: true, kind: Kind::Bytes, ..Default::default() },
//         }

//         value_optional {
//             expr: |_| Sha1Fn { value: Box::new(Noop) },
//             def: TypeDef { fallible: true, kind: Kind::Bytes, ..Default::default() },
//         }
//     ];

//     #[test]
//     fn sha1() {
//         let cases = vec![(
//             map!["foo": "foo"],
//             Ok(Value::from("0beec7b5ea3f0fdbc95d0dd47f3c5bc275da8a33")),
//             Sha1Fn::new(Box::new(Path::from("foo"))),
//         )];

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
