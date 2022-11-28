use ::value::Value;
use hmac::{Hmac as HmacHasher, Mac};
use sha1::Sha1;
use sha_2::{Sha224, Sha256, Sha384, Sha512};
use vrl::prelude::*;

macro_rules! hmac {
    ($algorithm:ty, $key:expr, $val:expr) => {{
        let mut mac = <HmacHasher<$algorithm>>::new_from_slice($key.as_ref()).expect("key is bytes");
        mac.update($val.as_ref());
        let result = mac.finalize();
        let code_bytes = result.into_bytes();
        code_bytes.to_vec()
    }};
}

fn hmac(value: Value, key: Value, algorithm: &Bytes, encoding: &Bytes) -> Resolved {
    let value = value.try_bytes()?;
    let key = key.try_bytes()?;

    let code_bytes = match algorithm.as_ref() {
        b"SHA1" => hmac!(Sha1, key, value),
        b"SHA-224" => hmac!(Sha224, key, value),
        b"SHA-256" => hmac!(Sha256, key, value),
        b"SHA-384" => hmac!(Sha384, key, value),
        b"SHA-512" => hmac!(Sha512, key, value),
        _ => unreachable!("enum invariant")
    };

    let hash = match encoding.as_ref() {
        b"hex" => hex::encode(code_bytes),
        b"base64" => base64::encode(code_bytes),
        _ => unreachable!("enum invariant")
    };
    Ok(hash.into())
}

fn encode_formats() -> Vec<Value> {
    vec![
        value!("hex"),
        value!("base64")
    ]
}

fn algorithms() -> Vec<Value> {
    vec![
        value!("SHA1"),
        value!("SHA-224"),
        value!("SHA-256"),
        value!("SHA-384"),
        value!("SHA-512"),
    ]
}

#[derive(Clone, Copy, Debug)]
pub struct Hmac;

impl Function for Hmac {
    fn identifier(&self) -> &'static str {
        "hmac"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::ANY,
                required: true,
            },

            Parameter {
                keyword: "key",
                kind: kind::ANY,
                required: true,
            },

            Parameter {
                keyword: "algorithm",
                kind: kind::ANY,
                required: false
            },

            Parameter {
                keyword: "encoding",
                kind: kind::ANY,
                required: false
            }
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "default (SHA-256, base64-encoded result)",
                source: r#"hmac("Hello there", "supersecretkey")"#,
                result: Ok("kmpc79vrb6SODvg4LwivUnb443+IhR9SSW55KcBPKo8="),
            },

            Example {
                title: "SHA-256, hex encoded result",
                source: r#"hmac("Hello there", "supersecretkey", encoding: "hex")"#,
                result: Ok("926a5cefdbeb6fa48e0ef8382f08af5276f8e37f88851f52496e7929c04f2a8f")
            },

            Example {
                title: "SHA1, base64-encoded result",
                source: r#"hmac("Hello there", "supersecretkey", algorithm: "SHA1")"#,
                result: Ok("795HKoopDtOb45EOUroxeHz1OWo=")
            }
        ]
    }

    fn compile(
        &self,
        _state: &state::TypeState,
        _ctx: &mut FunctionCompileContext,
        arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");
        let key = arguments.required("key");
        let algorithm = arguments
            .optional_enum("algorithm", &algorithms())?
            .unwrap_or_else(|| value!("SHA-256"))
            .try_bytes()
            .expect("algorithm not bytes");
        let encoding = arguments
            .optional_enum("encoding", &encode_formats())?
            .unwrap_or_else(|| value!("base64"))
            .try_bytes()
            .expect("encoding not bytes");

        Ok(HmacFn { value, key, algorithm, encoding }.as_expr())
    }
}

#[derive(Debug, Clone)]
struct HmacFn {
    value: Box<dyn Expression>,
    key: Box<dyn Expression>,
    algorithm: Bytes,
    encoding: Bytes,
}

impl FunctionExpression for HmacFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        let key = self.key.resolve(ctx)?;
        let algorithm = &self.algorithm;
        let encoding = &self.encoding;

        hmac(value, key, algorithm, encoding)
    }

    fn type_def(&self, _: &state::TypeState) -> TypeDef {
        TypeDef::bytes().fallible()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        hmac => Hmac;

        hmac {
            args: func_args![key: "supersecretkey", value: "Hello there"],
            want: Ok(value!("kmpc79vrb6SODvg4LwivUnb443+IhR9SSW55KcBPKo8=")),
            tdef: TypeDef::bytes().fallible(),
        }

        hmac_hex {
            args: func_args![key: "supersecretkey", value: "Hello there", encoding: "hex"],
            want: Ok(value!("926a5cefdbeb6fa48e0ef8382f08af5276f8e37f88851f52496e7929c04f2a8f")),
            tdef: TypeDef::bytes().fallible(),
        }

        hmac_sha1 {
            args: func_args![key: "supersecretkey", value: "Hello there", algorithm: "SHA1"],
            want: Ok(value!("795HKoopDtOb45EOUroxeHz1OWo=")),
            tdef: TypeDef::bytes().fallible(),
        }

        hmac_sha1_hex {
            args: func_args![key: "supersecretkey", value: "Hello there", algorithm: "SHA1", encoding: "hex"],
            want: Ok(value!("efde472a8a290ed39be3910e52ba31787cf5396a")),
            tdef: TypeDef::bytes().fallible(),
        }

        hmac_sha224 {
            args: func_args![key: "supersecretkey", value: "Hello there", algorithm: "SHA-224"],
            want: Ok(value!("XjIEvHrDISF42yzL5xXTcUSC3W9iXeGdGWgjgA==")),
            tdef: TypeDef::bytes().fallible(),
        }

        hmac_sha224_hex {
            args: func_args![key: "supersecretkey", value: "Hello there", algorithm: "SHA-224", encoding: "hex"],
            want: Ok(value!("5e3204bc7ac3212178db2ccbe715d3714482dd6f625de19d19682380")),
            tdef: TypeDef::bytes().fallible(),
        }

        hmac_sha384 {
            args: func_args![key: "supersecretkey", value: "Hello there", algorithm: "SHA-384"],
            want: Ok(value!("KSUHHqzGBNqYp2g8AEFtjnFK2L3KxbBoPx5G5siNuFuKI4bDhnzWE28O09JghdWQ")),
            tdef: TypeDef::bytes().fallible(),
        }

        hmac_sha384_hex {
            args: func_args![key: "supersecretkey", value: "Hello there", algorithm: "SHA-384", encoding: "hex"],
            want: Ok(value!("2925071eacc604da98a7683c00416d8e714ad8bdcac5b0683f1e46e6c88db85b8a2386c3867cd6136f0ed3d26085d590")),
            tdef: TypeDef::bytes().fallible(),
        }

        hmac_sha512 {
            args: func_args![key: "supersecretkey", value: "Hello there", algorithm: "SHA-512"],
            want: Ok(value!("8hyG6QeWoMOeD9/Ys5EDmlFc2szaMoTzV4uXJ8HCxZ8WqiR4gJCEgDs0MnRBih05M2mZCbwuvDunUx1SbDQDSQ==")),
            tdef: TypeDef::bytes().fallible(),
        }

        hmac_sha512_hex {
            args: func_args![key: "supersecretkey", value: "Hello there", algorithm: "SHA-512", encoding: "hex"],
            want: Ok(value!("f21c86e90796a0c39e0fdfd8b391039a515cdaccda3284f3578b9727c1c2c59f16aa2478809084803b343274418a1d3933699909bc2ebc3ba7531d526c340349")),
            tdef: TypeDef::bytes().fallible(),
        }
    ];
}
