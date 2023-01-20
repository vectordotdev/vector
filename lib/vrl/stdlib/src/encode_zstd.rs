use ::value::Value;
use nom::AsBytes;
use std::io::Read;
use vrl::prelude::expression::FunctionExpression;
use vrl::prelude::*;

fn encode_zstd(value: Value, compression_level: Option<Value>) -> Resolved {
    let compression_level = match compression_level {
        None => 0,
        Some(value) => value.try_integer()? as i32,
    };

    let value = value.try_bytes()?;
    let result = zstd::encode_all(value.as_bytes(), compression_level);

    match result {
        Ok(encoded_bytes) => Ok(Value::Bytes(encoded_bytes.into())),
        Err(_) => Err("unable to encode value with Zstd encoder".into()),
    }
}

#[derive(Clone, Copy, Debug)]
pub struct EncodeZstd;

impl Function for EncodeZstd {
    fn identifier(&self) -> &'static str {
        "encode_zstd"
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "demo string",
            source: r#"encode_base64(encode_zstd!("encode_me"))"#,
            result: Ok("H4sIAAAAAAAA/0vNS85PSY3PTQUAN7ZBnAkAAAA="),
        }]
    }

    fn compile(
        &self,
        _state: &state::TypeState,
        _ctx: &mut FunctionCompileContext,
        arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");
        let compression_level = arguments.optional("compression_level");

        Ok(EncodeZstdFn {
            value,
            compression_level,
        }
        .as_expr())
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::BYTES,
                required: true,
            },
            Parameter {
                keyword: "compression_level",
                kind: kind::INTEGER,
                required: false,
            },
        ]
    }
}

#[derive(Clone, Debug)]
struct EncodeZstdFn {
    value: Box<dyn Expression>,
    compression_level: Option<Box<dyn Expression>>,
}

impl FunctionExpression for EncodeZstdFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;

        let compression_level = self
            .compression_level
            .as_ref()
            .map(|expr| expr.resolve(ctx))
            .transpose()?;

        encode_zstd(value, compression_level)
    }

    fn type_def(&self, _: &state::TypeState) -> TypeDef {
        // Always fallible due to the possibility of encoding errors that VRL can't detect
        TypeDef::bytes().fallible()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use base64::Engine;

    fn decode_base64(text: &str) -> Vec<u8> {
        let engine = base64::engine::GeneralPurpose::new(
            &base64::alphabet::STANDARD,
            base64::engine::general_purpose::GeneralPurposeConfig::new(),
        );

        engine.decode(text).expect("Cannot decode from Base64")
    }

    test_function![
        encode_zstd => EncodeZstd;

        with_defaults {
            args: func_args![value: value!("encode_me")],
            want: Ok(value!(decode_base64("H4sIAAAAAAAA/0vNS85PSY3PTQUAN7ZBnAkAAAA=").as_bytes())),
            tdef: TypeDef::bytes().fallible(),
        }

        with_custom_compression_level {
            args: func_args![value: value!("encode_me"), compression_level: 2],
            want: Ok(value!(decode_base64("H4sIAAAAAAAA/0vNS85PSY3PTQUAN7ZBnAkAAAA=").as_bytes())),
            tdef: TypeDef::bytes().fallible(),
        }
    ];
}
