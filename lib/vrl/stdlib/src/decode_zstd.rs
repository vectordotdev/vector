use ::value::Value;
use std::io::Read;
use vrl::prelude::expression::FunctionExpression;
use vrl::prelude::*;

fn decode_zstd(value: Value) -> Resolved {
    let value = value.try_bytes()?;
    let result = zstd::decode_all(value);

    match result {
        Ok(decoded_bytes) => Ok(Value::Bytes(decoded_bytes.into())),
        Err(_) => Err("unable to decode value with Zstd decoder".into()),
    }
}

#[derive(Clone, Copy, Debug)]
pub struct DecodeZstd;

impl Function for DecodeZstd {
    fn identifier(&self) -> &'static str {
        "decode_zstd"
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "demo string",
            source: r#"decode_zstd!(decode_base64!("H4sIAB8BymMAAyvISU0sTlVISU3OT0lVyE0FAJsZ870QAAAA"))"#,
            result: Ok(r#"please decode me"#),
        }]
    }

    fn compile(
        &self,
        _state: &state::TypeState,
        _ctx: &mut FunctionCompileContext,
        arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");

        Ok(DecodeZstdFn { value }.as_expr())
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            kind: kind::BYTES,
            required: true,
        }]
    }
}

#[derive(Clone, Debug)]
struct DecodeZstdFn {
    value: Box<dyn Expression>,
}

impl FunctionExpression for DecodeZstdFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;

        decode_zstd(value)
    }

    fn type_def(&self, _: &state::TypeState) -> TypeDef {
        // Always fallible due to the possibility of decoding errors that VRL can't detect
        TypeDef::bytes().fallible()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::read::GzEncoder;
    use nom::AsBytes;

    fn get_encoded_bytes(text: &str) -> Vec<u8> {
        let mut buf = Vec::new();
        let mut gz = GzEncoder::new(text.as_bytes(), flate2::Compression::fast());
        gz.read_to_end(&mut buf)
            .expect("Cannot encode bytes with Gzip encoder");
        buf
    }

    test_function![
        decode_zstd => DecodeZstd;

        right_zstd {
            args: func_args![value: value!(get_encoded_bytes("sample").as_bytes())],
            want: Ok(value!(b"sample")),
            tdef: TypeDef::bytes().fallible(),
        }

        wrong_zstd {
            args: func_args![value: value!("some_bytes")],
            want: Err("unable to decode value with Gzip decoder"),
            tdef: TypeDef::bytes().fallible(),
        }
    ];
}
