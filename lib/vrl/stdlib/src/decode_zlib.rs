use ::value::Value;
use flate2::read::ZlibDecoder;
use std::io::Read;
use vrl::prelude::expression::FunctionExpression;
use vrl::prelude::*;

fn decode_zlib(value: Value) -> Resolved {
    let value = value.try_bytes()?;
    let mut buf = Vec::new();
    let result = ZlibDecoder::new(std::io::Cursor::new(value)).read_to_end(&mut buf);

    match result {
        Ok(_) => Ok(Value::Bytes(buf.into())),
        Err(_) => Err("unable to decode value with Zlib decoder".into()),
    }
}

#[derive(Clone, Copy, Debug)]
pub struct DecodeZlib;

impl Function for DecodeZlib {
    fn identifier(&self) -> &'static str {
        "decode_zlib"
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "demo string",
            source: r#"decode_zlib!(decode_base64!("eJxLzUvOT0mNz00FABI5A6A="))"#,
            result: Ok(r#"encode_me"#),
        }]
    }

    fn compile(
        &self,
        _state: &state::TypeState,
        _ctx: &mut FunctionCompileContext,
        arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");

        Ok(DecodeZlibFn { value }.as_expr())
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
struct DecodeZlibFn {
    value: Box<dyn Expression>,
}

impl FunctionExpression for DecodeZlibFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;

        decode_zlib(value)
    }

    fn type_def(&self, _: &state::TypeState) -> TypeDef {
        // Always fallible due to the possibility of decoding errors that VRL can't detect
        TypeDef::bytes().fallible()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::read::ZlibEncoder;
    use nom::AsBytes;

    fn get_encoded_bytes(text: &str) -> Vec<u8> {
        let mut buf = Vec::new();
        let mut gz = ZlibEncoder::new(text.as_bytes(), flate2::Compression::fast());
        gz.read_to_end(&mut buf)
            .expect("Cannot encode bytes with Gzip encoder");
        buf
    }

    test_function![
        decode_zlib => DecodeZlib;

        right_gzip {
            args: func_args![value: value!(get_encoded_bytes("sample").as_bytes())],
            want: Ok(value!(b"sample")),
            tdef: TypeDef::bytes().fallible(),
        }

        wrong_gzip {
            args: func_args![value: value!("some_bytes")],
            want: Err("unable to decode value with Zlib decoder"),
            tdef: TypeDef::bytes().fallible(),
        }
    ];
}
