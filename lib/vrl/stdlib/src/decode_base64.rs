use std::str::FromStr;

use ::value::Value;
use base64::Engine as _;
use vrl::prelude::expression::FunctionExpression;
use vrl::prelude::*;

use crate::util::Base64Charset;

fn decode_base64(charset: Option<Value>, value: Value) -> Resolved {
    let charset = charset
        .map(Value::try_bytes)
        .transpose()?
        .map(|c| Base64Charset::from_str(&String::from_utf8_lossy(&c)))
        .transpose()?
        .unwrap_or_default();
    let alphabet = match charset {
        Base64Charset::Standard => base64::alphabet::STANDARD,
        Base64Charset::UrlSafe => base64::alphabet::URL_SAFE,
    };
    let value = value.try_bytes()?;
    let engine = base64::engine::GeneralPurpose::new(
        &alphabet,
        base64::engine::general_purpose::GeneralPurposeConfig::new(),
    );

    match engine.decode(value) {
        Ok(s) => Ok(Value::from(Bytes::from(s))),
        Err(_) => Err("unable to decode value to base64".into()),
    }
}

#[derive(Clone, Copy, Debug)]
pub struct DecodeBase64;

impl Function for DecodeBase64 {
    fn identifier(&self) -> &'static str {
        "decode_base64"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::BYTES,
                required: true,
            },
            Parameter {
                keyword: "charset",
                kind: kind::BYTES,
                required: false,
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
        let charset = arguments.optional("charset");

        Ok(DecodeBase64Fn { value, charset }.as_expr())
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "demo string",
            source: r#"decode_base64!("c29tZSBzdHJpbmcgdmFsdWU=")"#,
            result: Ok(r#"some string value"#),
        }]
    }
}

#[derive(Clone, Debug)]
struct DecodeBase64Fn {
    value: Box<dyn Expression>,
    charset: Option<Box<dyn Expression>>,
}

impl FunctionExpression for DecodeBase64Fn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        let charset = self.charset.as_ref().map(|c| c.resolve(ctx)).transpose()?;

        decode_base64(charset, value)
    }

    fn type_def(&self, _: &state::TypeState) -> TypeDef {
        // Always fallible due to the possibility of decoding errors that VRL can't detect in
        // advance: https://docs.rs/base64/0.13.0/base64/enum.DecodeError.html
        TypeDef::bytes().fallible()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    test_function![
        decode_base64 => DecodeBase64;

        with_defaults {
            args: func_args![value: value!("c29tZSs9c3RyaW5nL3ZhbHVl")],
            want: Ok(value!("some+=string/value")),
            tdef: TypeDef::bytes().fallible(),
        }

        with_standard_charset {
            args: func_args![value: value!("c29tZSs9c3RyaW5nL3ZhbHVl"), charset: value!["standard"]],
            want: Ok(value!("some+=string/value")),
            tdef: TypeDef::bytes().fallible(),
        }

        with_urlsafe_charset {
            args: func_args![value: value!("c29tZSs9c3RyaW5nL3ZhbHVl"), charset: value!("url_safe")],
            want: Ok(value!("some+=string/value")),
            tdef: TypeDef::bytes().fallible(),
        }

        empty_string_standard_charset {
            args: func_args![value: value!(""), charset: value!("standard")],
            want: Ok(value!("")),
            tdef: TypeDef::bytes().fallible(),
        }

        empty_string_urlsafe_charset {
            args: func_args![value: value!(""), charset: value!("url_safe")],
            want: Ok(value!("")),
            tdef: TypeDef::bytes().fallible(),
        }
    ];
}
