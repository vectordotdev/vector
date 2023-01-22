use ::value::Value;
use nom::AsBytes;
use std::str;
use vrl::prelude::expression::FunctionExpression;
use vrl::prelude::*;

fn decode_base16(value: Value) -> Resolved {
    match base16::decode(&value.try_bytes_utf8_lossy()?.to_string()) {
        Ok(s) => Ok((s.as_bytes()).into()),
        Err(_) => Err("unable to decode value to base16".into()),
    }
}

#[derive(Clone, Copy, Debug)]
pub struct DecodeBase16;

impl Function for DecodeBase16 {
    fn identifier(&self) -> &'static str {
        "decode_base16"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            kind: kind::BYTES,
            required: true,
        }]
    }

    fn compile(
        &self,
        _state: &state::TypeState,
        _ctx: &mut FunctionCompileContext,
        arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");

        Ok(DecodeBase16Fn { value }.as_expr())
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "demo string",
            source: r#"decode_base16!("736F6D6520737472696E672076616C7565")"#,
            result: Ok(r#"some string value"#),
        }]
    }
}

#[derive(Clone, Debug)]
struct DecodeBase16Fn {
    value: Box<dyn Expression>,
}

impl FunctionExpression for DecodeBase16Fn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;

        decode_base16(value)
    }

    fn type_def(&self, _: &state::TypeState) -> TypeDef {
        // Always fallible due to the possibility of decoding errors that VRL can't detect in `base16`
        TypeDef::bytes().fallible()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    test_function![
        decode_base16 => DecodeBase16;

        standard {
            args: func_args![value: value!("736F6D652B3D737472696E672F76616C7565")],
            want: Ok(value!("some+=string/value")),
            tdef: TypeDef::bytes().fallible(),
        }
    ];
}
