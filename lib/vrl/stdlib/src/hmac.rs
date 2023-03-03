use base64::Engine;
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
        "base64" => base64::engine::general_purpose::STANDARD.encode(code_bytes),
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
                source: r#"hmac("Hello there", "super-secret-key")"#,
                result: Ok("eLGE8YMviv85NPXgISRUZxstBNSU47JQdcXkUWcClmI="),
            },

            Example {
                title: "SHA-256, hex encoded result",
                source: r#"hmac("Hello there", "super-secret-key", encoding: "hex")"#,
                result: Ok("78b184f1832f8aff3934f5e0212454671b2d04d494e3b25075c5e45167029662")
            },

            Example {
                title: "SHA1, base64-encoded result",
                source: r#"hmac("Hello there", "super-secret-key", algorithm: "SHA1")"#,
                result: Ok("MiyBIHO8Set9+6crALiwkS0yFPE=")
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
        let valid_algorithms = vec!["SHA1", "SHA-224", "SHA-256", "SHA-384", "SHA-512"];
        let valid_encodings = vec!["base64", "hex"];

        let mut valid_static_algo = false;
        let mut valid_static_enc = false;
        if let Some(algorithm) = self.algorithm.as_ref() {
            if let Some(algorithm) = algorithm.as_value() {
                if let Ok(algorithm) = algorithm.try_bytes_utf8_lossy() {
                    let algorithm = algorithm.to_uppercase();
                    valid_static_algo = valid_algorithms.contains(&algorithm.as_str());
                }
            }
        } else { valid_static_algo = true }

        if let Some(encoding) = self.encoding.as_ref() {
            if let Some(encoding) = encoding.as_value() {
                if let Ok(encoding) = encoding.try_bytes_utf8_lossy() {
                    let encoding = encoding.to_lowercase();
                    valid_static_enc = valid_encodings.contains(&encoding.as_str());
                }
            }
        } else { valid_static_enc = true }

        if valid_static_algo && valid_static_enc {
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
            want: Ok(value!("eLGE8YMviv85NPXgISRUZxstBNSU47JQdcXkUWcClmI=")),
            tdef: TypeDef::bytes().infallible(),
        }

        hmac_hex {
            args: func_args![key: "super-secret-key", value: "Hello there", encoding: "hex"],
            want: Ok(value!("78b184f1832f8aff3934f5e0212454671b2d04d494e3b25075c5e45167029662")),
            tdef: TypeDef::bytes().infallible(),
        }

        hmac_sha1 {
            args: func_args![key: "super-secret-key", value: "Hello there", algorithm: "SHA1"],
            want: Ok(value!("MiyBIHO8Set9+6crALiwkS0yFPE=")),
            tdef: TypeDef::bytes().infallible(),
        }

        hmac_sha1_hex {
            args: func_args![key: "super-secret-key", value: "Hello there", algorithm: "SHA1", encoding: "hex"],
            want: Ok(value!("322c812073bc49eb7dfba72b00b8b0912d3214f1")),
            tdef: TypeDef::bytes().infallible(),
        }

        hmac_sha224 {
            args: func_args![key: "super-secret-key", value: "Hello there", algorithm: "SHA-224"],
            want: Ok(value!("QvzLwrfSKhQ7kvJlqARhh1WKlNEd27MGIiB+kA==")),
            tdef: TypeDef::bytes().infallible(),
        }

        hmac_sha224_hex {
            args: func_args![key: "super-secret-key", value: "Hello there", algorithm: "SHA-224", encoding: "hex"],
            want: Ok(value!("42fccbc2b7d22a143b92f265a8046187558a94d11ddbb30622207e90")),
            tdef: TypeDef::bytes().infallible(),
        }

        hmac_sha384 {
            args: func_args![key: "super-secret-key", value: "Hello there", algorithm: "SHA-384"],
            want: Ok(value!("4lE3xNfeosy5JiNg9XOITVuBjz0Nt5KXNj9mQpTziPD5tYwEwR2IBrVguA3gP+0N")),
            tdef: TypeDef::bytes().infallible(),
        }

        hmac_sha384_hex {
            args: func_args![key: "super-secret-key", value: "Hello there", algorithm: "SHA-384", encoding: "hex"],
            want: Ok(value!("e25137c4d7dea2ccb9262360f573884d5b818f3d0db79297363f664294f388f0f9b58c04c11d8806b560b80de03fed0d")),
            tdef: TypeDef::bytes().infallible(),
        }

        hmac_sha512 {
            args: func_args![key: "super-secret-key", value: "Hello there", algorithm: "SHA-512"],
            want: Ok(value!("IMkqB2si80Mr/pGN/kMU0CQ8hQhkOrHX13mlZYSBzi/UCCEEQBDpeME2UX9Y/8jmwfJYMHOIWDA88KcQc8YOlg==")),
            tdef: TypeDef::bytes().infallible(),
        }

        hmac_sha512_hex {
            args: func_args![key: "super-secret-key", value: "Hello there", algorithm: "SHA-512", encoding: "hex"],
            want: Ok(value!("20c92a076b22f3432bfe918dfe4314d0243c8508643ab1d7d779a5658481ce2fd40821044010e978c136517f58ffc8e6c1f25830738858303cf0a71073c60e96")),
            tdef: TypeDef::bytes().infallible(),
        }
    ];
}
