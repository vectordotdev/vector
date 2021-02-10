use md5::Digest;
use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Md5;

impl Function for Md5 {
    fn identifier(&self) -> &'static str {
        "md5"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            kind: kind::ANY,
            required: true,
        }]
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "md5",
            source: r#"md5("foobar")"#,
            result: Ok("3858f62230ac3c915f300c664312c63f"),
        }]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");

        Ok(Box::new(Md5Fn { value }))
    }
}

#[derive(Debug, Clone)]
struct Md5Fn {
    value: Box<dyn Expression>,
}

impl Expression for Md5Fn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?.unwrap_bytes();

        Ok(hex::encode(md5::Md5::digest(&value)).into())
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().infallible().bytes()
    }
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use crate::map;

//     vrl::test_type_def![
//         value_string {
//             expr: |_| Md5Fn { value: Literal::from("foo").boxed() },
//             def: TypeDef { kind: Kind::Bytes, ..Default::default() },
//         }

//         value_non_string {
//             expr: |_| Md5Fn { value: Literal::from(1).boxed() },
//             def: TypeDef { fallible: true, kind: Kind::Bytes, ..Default::default() },
//         }

//         value_optional {
//             expr: |_| Md5Fn { value: Box::new(Noop) },
//             def: TypeDef { fallible: true, kind: Kind::Bytes, ..Default::default() },
//         }
//     ];

//     #[test]
//     fn md5() {
//         let cases = vec![(
//             map!["foo": "foo"],
//             Ok(Value::from("acbd18db4cc2f85cedef654fccc4a4d8")),
//             Md5Fn::new(Box::new(Path::from("foo"))),
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
