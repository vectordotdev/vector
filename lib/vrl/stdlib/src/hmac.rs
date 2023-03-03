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

fn hmac(value: Value, key: Value, algorithm: Value, encoding: Value) -> Resolved {
    let value = value.try_bytes()?;
    let key = key.try_bytes()?;
    let algorithm = algorithm.try_bytes_utf8_lossy()?.as_ref().to_uppercase();
    let encoding = encoding.try_bytes_utf8_lossy()?.as_ref().to_lowercase();

    let code_bytes = match algorithm.as_str() {
        "SHA1" => hmac!(Sha1, key, value),
        "SHA-224" => hmac!(Sha224, key, value),
        "SHA-256" => hmac!(Sha256, key, value),
        "SHA-384" => hmac!(Sha384, key, value),
        "SHA-512" => hmac!(Sha512, key, value),
        _ => return Err(format!("Invalid algorithm: {}", algorithm).into())
    };

    let hash = match encoding.as_str() {
        "hex" => hex::encode(code_bytes),
        "base64" => base64::encode(code_bytes),
        _ => return Err(format!("Invalid encoding: {}", encoding).into())
    };
    Ok(hash.into())
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
                kind: kind::BYTES,
                required: true,
            },

            Parameter {
                keyword: "key",
                kind: kind::BYTES,
                required: true,
            },

            Parameter {
                keyword: "algorithm",
                kind: kind::BYTES,
                required: false
            },

            Parameter {
                keyword: "encoding",
                kind: kind::BYTES,
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
        let algorithm = arguments.optional("algorithm");
        let encoding = arguments.optional("encoding");

        Ok(HmacFn { value, key, algorithm, encoding }.as_expr())
    }
}

#[derive(Debug, Clone)]
struct HmacFn {
    value: Box<dyn Expression>,
    key: Box<dyn Expression>,
    algorithm: Option<Box<dyn Expression>>,
    encoding: Option<Box<dyn Expression>>,
}

impl FunctionExpression for HmacFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        let key = self.key.resolve(ctx)?;
        let algorithm = match &self.algorithm {
            Some(algorithm) => algorithm.resolve(ctx)?,
            None => value!("SHA-256")
        };
        let encoding = match &self.encoding {
            Some(encoding) => encoding.resolve(ctx)?,
            None => value!("base64")
        };

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
