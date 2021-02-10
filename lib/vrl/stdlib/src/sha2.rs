use sha_2::{Digest, Sha224, Sha256, Sha384, Sha512, Sha512Trunc224, Sha512Trunc256};
use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Sha2;

impl Function for Sha2 {
    fn identifier(&self) -> &'static str {
        "sha2"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::BYTES,
                required: true,
            },
            Parameter {
                keyword: "variant",
                kind: kind::BYTES,
                required: false,
            },
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "default variant",
                source: r#"sha2("foobar")"#,
                result: Ok("d014c752bc2be868e16330f47e0c316a5967bcbc9c286a457761d7055b9214ce"),
            },
            Example {
                title: "custom variant",
                source: r#"sha2("foobar", "SHA-384")"#,
                result: Ok("3c9c30d9f665e74d515c842960d4a451c83a0125fd3de7392d7b37231af10c72ea58aedfcdf89a5765bf902af93ecf06"),
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let variants = vec![
            value!("SHA-224"),
            value!("SHA-256"),
            value!("SHA-384"),
            value!("SHA-512"),
            value!("SHA-512/224"),
            value!("SHA-512/256"),
        ];

        let value = arguments.required("value");
        let variant = arguments
            .optional_enum("variant", &variants)?
            .unwrap_or_else(|| value!("SHA-512/256"))
            .unwrap_bytes();

        Ok(Box::new(Sha2Fn { value, variant }))
    }
}

#[derive(Debug, Clone)]
struct Sha2Fn {
    value: Box<dyn Expression>,
    variant: Bytes,
}

impl Expression for Sha2Fn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?.unwrap_bytes();

        let hash = match self.variant.as_ref() {
            b"SHA-224" => encode::<Sha224>(&value),
            b"SHA-256" => encode::<Sha256>(&value),
            b"SHA-384" => encode::<Sha384>(&value),
            b"SHA-512" => encode::<Sha512>(&value),
            b"SHA-512/224" => encode::<Sha512Trunc224>(&value),
            b"SHA-512/256" => encode::<Sha512Trunc256>(&value),
            _ => unreachable!("enum invariant"),
        };

        Ok(hash.into())
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().infallible().bytes()
    }
}

#[inline]
fn encode<T: Digest>(value: &[u8]) -> String {
    hex::encode(T::digest(value))
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use crate::map;

//     vrl::test_type_def![
//         value_string {
//             expr: |_| Sha2Fn {
//                 value: Literal::from("foo").boxed(),
//                 variant: None,
//             },
//             def: TypeDef { kind: Kind::Bytes, ..Default::default() },
//         }

//         value_non_string {
//             expr: |_| Sha2Fn {
//                 value: Literal::from(1).boxed(),
//                 variant: None,
//             },
//             def: TypeDef { fallible: true, kind: Kind::Bytes, ..Default::default() },
//         }

//         value_optional {
//             expr: |_| Sha2Fn {
//                 value: Box::new(Noop),
//                 variant: None,
//             },
//             def: TypeDef { fallible: true, kind: Kind::Bytes, ..Default::default() },
//         }
//     ];

//     #[test]
//     fn sha2() {
//         let cases = vec![
//             (
//                 map!["foo": "foo"],
//                 Ok("d58042e6aa5a335e03ad576c6a9e43b41591bfd2077f72dec9df7930e492055d".into()),
//                 Sha2Fn::new(Box::new(Path::from("foo")), None),
//             ),
//             (
//                 map![],
//                 Ok("0808f64e60d58979fcb676c96ec938270dea42445aeefcd3a4e6f8db".into()),
//                 Sha2Fn::new(Box::new(Literal::from("foo")), Some("SHA-224")),
//             ),
//             (
//                 map![],
//                 Ok("2c26b46b68ffc68ff99b453c1d30413413422d706483bfa0f98a5e886266e7ae".into()),
//                 Sha2Fn::new(Box::new(Literal::from("foo")), Some("SHA-256")),
//             ),
//             (
//                 map![],
//                 Ok("98c11ffdfdd540676b1a137cb1a22b2a70350c9a44171d6b1180c6be5cbb2ee3f79d532c8a1dd9ef2e8e08e752a3babb".into()),
//                 Sha2Fn::new(Box::new(Literal::from("foo")), Some("SHA-384")),
//             ),
//             (
//                 map![],
//                 Ok("f7fbba6e0636f890e56fbbf3283e524c6fa3204ae298382d624741d0dc6638326e282c41be5e4254d8820772c5518a2c5a8c0c7f7eda19594a7eb539453e1ed7".into()),
//                 Sha2Fn::new(Box::new(Literal::from("foo")), Some("SHA-512")),
//             ),
//             (
//                 map![],
//                 Ok("d68f258d37d670cfc1ec1001a0394784233f88f056994f9a7e5e99be".into()),
//                 Sha2Fn::new(Box::new(Literal::from("foo")), Some("SHA-512/224")),
//             ),
//             (
//                 map![],
//                 Ok("d58042e6aa5a335e03ad576c6a9e43b41591bfd2077f72dec9df7930e492055d".into()),
//                 Sha2Fn::new(Box::new(Literal::from("foo")), Some("SHA-512/256")),
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
