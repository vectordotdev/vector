use ::value::Value;
use hmac::{Hmac as HmacHasher, Mac};
use sha1::Sha1;
use sha_2::{Sha224, Sha256, Sha384, Sha512};
use vrl::prelude::*;

macro_rules! hmac {
    ($algorithm:ty, $key:expr, $val:expr) => {{
        let mut mac =
            <HmacHasher<$algorithm>>::new_from_slice($key.as_ref()).expect("key is bytes");
        mac.update($val.as_ref());
        let result = mac.finalize();
        let code_bytes = result.into_bytes();
        code_bytes.to_vec()
    }};
}

fn hmac(value: Value, key: Value, algorithm: Value) -> Resolved {
    let value = value.try_bytes()?;
    let key = key.try_bytes()?;
    let algorithm = algorithm.try_bytes_utf8_lossy()?.as_ref().to_uppercase();

    let code_bytes = match algorithm.as_str() {
        "SHA1" => hmac!(Sha1, key, value),
        "SHA-224" => hmac!(Sha224, key, value),
        "SHA-256" => hmac!(Sha256, key, value),
        "SHA-384" => hmac!(Sha384, key, value),
        "SHA-512" => hmac!(Sha512, key, value),
        _ => return Err(format!("Invalid algorithm: {algorithm}").into()),
    };

    Ok(Value::Bytes(Bytes::from(code_bytes)))
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
                required: false,
            },
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "default SHA-256",
                source: r#"encode_base64(hmac("Hello there", "super-secret-key"))"#,
                result: Ok("eLGE8YMviv85NPXgISRUZxstBNSU47JQdcXkUWcClmI="),
            },
            Example {
                title: "SHA1",
                source: r#"encode_base64(hmac("Hello there", "super-secret-key", algorithm: "SHA1"))"#,
                result: Ok("MiyBIHO8Set9+6crALiwkS0yFPE="),
            },
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

        Ok(HmacFn {
            value,
            key,
            algorithm,
        }
        .as_expr())
    }
}

#[derive(Debug, Clone)]
struct HmacFn {
    value: Box<dyn Expression>,
    key: Box<dyn Expression>,
    algorithm: Option<Box<dyn Expression>>,
}

impl FunctionExpression for HmacFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        let key = self.key.resolve(ctx)?;
        let algorithm = match &self.algorithm {
            Some(algorithm) => algorithm.resolve(ctx)?,
            None => value!("SHA-256"),
        };

        hmac(value, key, algorithm)
    }

    fn type_def(&self, _: &state::TypeState) -> TypeDef {
        let valid_algorithms = vec!["SHA1", "SHA-224", "SHA-256", "SHA-384", "SHA-512"];

        let mut valid_static_algo = false;
        if let Some(algorithm) = self.algorithm.as_ref() {
            if let Some(algorithm) = algorithm.as_value() {
                if let Ok(algorithm) = algorithm.try_bytes_utf8_lossy() {
                    let algorithm = algorithm.to_uppercase();
                    valid_static_algo = valid_algorithms.contains(&algorithm.as_str());
                }
            }
        } else {
            valid_static_algo = true
        }

        if valid_static_algo {
            TypeDef::bytes().infallible()
        } else {
            TypeDef::bytes().fallible()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        hmac => Hmac;

        hmac {
            args: func_args![key: "super-secret-key", value: "Hello there"],
            want: Ok(value!(b"x\xb1\x84\xf1\x83/\x8a\xff94\xf5\xe0!$Tg\x1b-\x04\xd4\x94\xe3\xb2Pu\xc5\xe4Qg\x02\x96b")),
            tdef: TypeDef::bytes().infallible(),
        }

        hmac_sha1 {
            args: func_args![key: "super-secret-key", value: "Hello there", algorithm: "SHA1"],
            want: Ok(value!(b"2,\x81 s\xbcI\xeb}\xfb\xa7+\x00\xb8\xb0\x91-2\x14\xf1")),
            tdef: TypeDef::bytes().infallible(),
        }

        hmac_sha224 {
            args: func_args![key: "super-secret-key", value: "Hello there", algorithm: "SHA-224"],
            want: Ok(value!(b"B\xfc\xcb\xc2\xb7\xd2*\x14;\x92\xf2e\xa8\x04a\x87U\x8a\x94\xd1\x1d\xdb\xb3\x06\" ~\x90")),
            tdef: TypeDef::bytes().infallible(),
        }

        hmac_sha384 {
            args: func_args![key: "super-secret-key", value: "Hello there", algorithm: "SHA-384"],
            want: Ok(value!(b"\xe2Q7\xc4\xd7\xde\xa2\xcc\xb9&#`\xf5s\x88M[\x81\x8f=\x0d\xb7\x92\x976?fB\x94\xf3\x88\xf0\xf9\xb5\x8c\x04\xc1\x1d\x88\x06\xb5`\xb8\x0d\xe0?\xed\x0d")),
            tdef: TypeDef::bytes().infallible(),
        }

        hmac_sha512 {
            args: func_args![key: "super-secret-key", value: "Hello there", algorithm: "SHA-512"],
            want: Ok(value!(b" \xc9*\x07k\"\xf3C+\xfe\x91\x8d\xfeC\x14\xd0$<\x85\x08d:\xb1\xd7\xd7y\xa5e\x84\x81\xce/\xd4\x08!\x04@\x10\xe9x\xc16Q\x7fX\xff\xc8\xe6\xc1\xf2X0s\x88X0<\xf0\xa7\x10s\xc6\x0e\x96")),
            tdef: TypeDef::bytes().infallible(),
        }
    ];
}
