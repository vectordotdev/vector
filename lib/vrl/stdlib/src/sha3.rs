use sha_3::{Digest, Sha3_224, Sha3_256, Sha3_384, Sha3_512};
use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Sha3;

impl Function for Sha3 {
    fn identifier(&self) -> &'static str {
        "sha3"
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
                source: r#"sha3("foobar")"#,
                result: Ok("ff32a30c3af5012ea395827a3e99a13073c3a8d8410a708568ff7e6eb85968fccfebaea039bc21411e9d43fdb9a851b529b9960ffea8679199781b8f45ca85e2"),
            },
            Example {
                title: "custom variant",
                source: r#"sha3("foobar", "SHA3-384")"#,
                result: Ok("0fa8abfbdaf924ad307b74dd2ed183b9a4a398891a2f6bac8fd2db7041b77f068580f9c6c66f699b496c2da1cbcc7ed8"),
            },
        ]
    }

    fn compile(&self, _state: &state::Compiler, mut arguments: ArgumentList) -> Compiled {
        let variants = vec![
            value!("SHA3-224"),
            value!("SHA3-256"),
            value!("SHA3-384"),
            value!("SHA3-512"),
        ];

        let value = arguments.required("value");
        let variant = arguments
            .optional_enum("variant", &variants)?
            .unwrap_or_else(|| value!("SHA3-512"))
            .try_bytes()
            .expect("variant not bytes");

        Ok(Box::new(Sha3Fn { value, variant }))
    }
}

#[derive(Debug, Clone)]
struct Sha3Fn {
    value: Box<dyn Expression>,
    variant: Bytes,
}

impl Expression for Sha3Fn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?.try_bytes()?;

        let hash = match self.variant.as_ref() {
            b"SHA3-224" => encode::<Sha3_224>(&value),
            b"SHA3-256" => encode::<Sha3_256>(&value),
            b"SHA3-384" => encode::<Sha3_384>(&value),
            b"SHA3-512" => encode::<Sha3_512>(&value),
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

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        sha3 => Sha3;

        sha3 {
             args: func_args![value: "foo"],
             want: Ok("4bca2b137edc580fe50a88983ef860ebaca36c857b1f492839d6d7392452a63c82cbebc68e3b70a2a1480b4bb5d437a7cba6ecf9d89f9ff3ccd14cd6146ea7e7"),
             tdef: TypeDef::new().infallible().bytes(),
         }

        sha3_224  {
            args: func_args![value: "foo"],
            want: Ok("4bca2b137edc580fe50a88983ef860ebaca36c857b1f492839d6d7392452a63c82cbebc68e3b70a2a1480b4bb5d437a7cba6ecf9d89f9ff3ccd14cd6146ea7e7"),
            tdef: TypeDef::new().infallible().bytes(),
        }

        sha3_256 {
             args: func_args![value: "foo",
                              variant: "SHA3-256"
             ],
             want: Ok("76d3bc41c9f588f7fcd0d5bf4718f8f84b1c41b20882703100b9eb9413807c01"),
             tdef: TypeDef::new().infallible().bytes(),
         }

        sha3_384 {
             args: func_args![value: "foo",
                              variant: "SHA3-384"
             ],
             want: Ok("665551928d13b7d84ee02734502b018d896a0fb87eed5adb4c87ba91bbd6489410e11b0fbcc06ed7d0ebad559e5d3bb5"),
             tdef: TypeDef::new().infallible().bytes(),
         }

        sha3_512 {
             args: func_args![value: "foo",
                              variant: "SHA3-512"
             ],
             want: Ok("4bca2b137edc580fe50a88983ef860ebaca36c857b1f492839d6d7392452a63c82cbebc68e3b70a2a1480b4bb5d437a7cba6ecf9d89f9ff3ccd14cd6146ea7e7"),
             tdef: TypeDef::new().infallible().bytes(),
         }
    ];
}
