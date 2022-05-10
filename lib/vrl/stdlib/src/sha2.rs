use sha_2::{Digest, Sha224, Sha256, Sha384, Sha512, Sha512_224, Sha512_256};
use vrl::prelude::*;

fn sha2(value: Value, variant: &Bytes) -> Resolved {
    let value = value.try_bytes()?;
    let hash = match variant.as_ref() {
        b"SHA-224" => encode::<Sha224>(&value),
        b"SHA-256" => encode::<Sha256>(&value),
        b"SHA-384" => encode::<Sha384>(&value),
        b"SHA-512" => encode::<Sha512>(&value),
        b"SHA-512/224" => encode::<Sha512_224>(&value),
        b"SHA-512/256" => encode::<Sha512_256>(&value),
        _ => unreachable!("enum invariant"),
    };
    Ok(hash.into())
}

#[derive(Clone, Copy, Debug)]
pub struct Sha2;

fn variants() -> Vec<Value> {
    vec![
        value!("SHA-224"),
        value!("SHA-256"),
        value!("SHA-384"),
        value!("SHA-512"),
        value!("SHA-512/224"),
        value!("SHA-512/256"),
    ]
}

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

    fn compile(
        &self,
        _state: (&mut state::LocalEnv, &mut state::ExternalEnv),
        _ctx: &mut FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");
        let variant = arguments
            .optional_enum("variant", &variants())?
            .unwrap_or_else(|| value!("SHA-512/256"))
            .try_bytes()
            .expect("variant not bytes");

        Ok(Box::new(Sha2Fn { value, variant }))
    }

    fn compile_argument(
        &self,
        _args: &[(&'static str, Option<FunctionArgument>)],
        _ctx: &mut FunctionCompileContext,
        name: &str,
        expr: Option<&expression::Expr>,
    ) -> CompiledArgument {
        match (name, expr) {
            ("variant", Some(expr)) => {
                let variant = expr
                    .as_enum("variant", variants())?
                    .try_bytes()
                    .expect("variant not bytes");

                Ok(Some(Box::new(variant) as _))
            }
            ("variant", None) => Ok(Some(Box::new(Bytes::from("SHA-512/256")) as _)),
            _ => Ok(None),
        }
    }

    fn call_by_vm(&self, _ctx: &mut Context, args: &mut VmArgumentList) -> Resolved {
        let value = args.required("value");
        let variant = args
            .required_any("variant")
            .downcast_ref::<Bytes>()
            .unwrap();

        sha2(value, variant)
    }
}

#[derive(Debug, Clone)]
struct Sha2Fn {
    value: Box<dyn Expression>,
    variant: Bytes,
}

impl Expression for Sha2Fn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        let variant = &self.variant;

        sha2(value, variant)
    }

    fn type_def(&self, _: (&state::LocalEnv, &state::ExternalEnv)) -> TypeDef {
        TypeDef::bytes().infallible()
    }
}

#[inline]
fn encode<T: Digest>(value: &[u8]) -> String {
    hex::encode(T::digest(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        sha2 => Sha2;

        sha2 {
             args: func_args![value: "foo"],
             want: Ok("d58042e6aa5a335e03ad576c6a9e43b41591bfd2077f72dec9df7930e492055d"),
             tdef: TypeDef::bytes().infallible(),
         }

        sha2_224 {
            args: func_args![value: "foo",
                             variant: "SHA-224"
            ],
            want: Ok("0808f64e60d58979fcb676c96ec938270dea42445aeefcd3a4e6f8db"),
            tdef: TypeDef::bytes().infallible(),
        }

        sha2_256 {
             args: func_args![value: "foo",
                              variant: "SHA-256"
             ],
             want: Ok("2c26b46b68ffc68ff99b453c1d30413413422d706483bfa0f98a5e886266e7ae"),
             tdef: TypeDef::bytes().infallible(),
         }

        sha2_385 {
            args: func_args![value: "foo",
                             variant: "SHA-384"
            ],
            want: Ok("98c11ffdfdd540676b1a137cb1a22b2a70350c9a44171d6b1180c6be5cbb2ee3f79d532c8a1dd9ef2e8e08e752a3babb"),
            tdef: TypeDef::bytes().infallible(),
        }

        sha2_512 {
             args: func_args![value: "foo",
                              variant: "SHA-512"
             ],
             want: Ok("f7fbba6e0636f890e56fbbf3283e524c6fa3204ae298382d624741d0dc6638326e282c41be5e4254d8820772c5518a2c5a8c0c7f7eda19594a7eb539453e1ed7"),
             tdef: TypeDef::bytes().infallible(),
         }

        sha2_512_224 {
             args: func_args![value: "foo",
                              variant: "SHA-512/224"
             ],
             want: Ok("d68f258d37d670cfc1ec1001a0394784233f88f056994f9a7e5e99be"),
             tdef: TypeDef::bytes().infallible(),
         }

        sha2_512_256 {
             args: func_args![value: "foo",
                              variant: "SHA-512/256"
             ],
             want: Ok("d58042e6aa5a335e03ad576c6a9e43b41591bfd2077f72dec9df7930e492055d"),
             tdef: TypeDef::bytes().infallible(),
         }
    ];
}
