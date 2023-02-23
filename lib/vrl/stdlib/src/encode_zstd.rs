use ::value::Value;
use nom::AsBytes;
use vrl::prelude::expression::FunctionExpression;
use vrl::prelude::*;

fn encode_zstd(value: Value, compression_level: Option<Value>) -> Resolved {
    let compression_level = match compression_level {
        None => 0,
        Some(value) => value.try_integer()? as i32,
    };

    let value = value.try_bytes()?;
    // Zstd encoding will not fail in the case of using `encode_all` function
    let encoded_bytes = zstd::encode_all(value.as_bytes(), compression_level)
        .expect("zstd compression failed, please report");

    Ok(Value::Bytes(encoded_bytes.into()))
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
            source: r#"encode_base64(encode_zstd("encode_me"))"#,
            result: Ok("KLUv/QBYSQAAZW5jb2RlX21l"),
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
        TypeDef::bytes().infallible()
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
            args: func_args![value: value!("you_have_successfully_decoded_me.congratulations.you_are_breathtaking.")],
            want: Ok(value!(decode_base64("KLUv/QBY/QEAYsQOFKClbQBedqXsb96EWDax/f/F/z+gNU4ZTInaUeAj82KqPFjUzKqhcfDqAIsLvAsnY1bI/N2mHzDixRQA").as_bytes())),
            tdef: TypeDef::bytes().infallible(),
        }

        with_custom_compression_level {
            args: func_args![value: value!("you_have_successfully_decoded_me.congratulations.you_are_breathtaking."), compression_level: 22],
            want: Ok(value!(decode_base64("KLUv/QCIFQIAIkQOFKClbQBedqXsb96EWDYp/f+l/x+hNU4ZrER9FNiRKw8WtVk1GgevDjBxgXdhyZgVMn+3aQ+Y2GIKAQBBAwUF").as_bytes())),
            tdef: TypeDef::bytes().infallible(),
        }
    ];
}
