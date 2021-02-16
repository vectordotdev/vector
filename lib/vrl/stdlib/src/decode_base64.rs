use crate::util::Base64Charset;
use std::str::FromStr;
use vrl::prelude::*;

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

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");
        let charset = arguments.optional("charset");

        Ok(Box::new(DecodeBase64Fn { value, charset }))
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

impl Expression for DecodeBase64Fn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?.unwrap_bytes();

        let charset = self
            .charset
            .as_ref()
            .map(|c| {
                c.resolve(ctx)
                    .and_then(|v| Value::try_bytes(v).map_err(Into::into))
            })
            .transpose()?
            .map(|c| Base64Charset::from_str(&String::from_utf8_lossy(&c)))
            .transpose()?
            .unwrap_or_default();

        let config = match charset {
            Base64Charset::Standard => base64::STANDARD,
            Base64Charset::UrlSafe => base64::URL_SAFE,
        };

        match base64::decode_config(value, config) {
            Ok(s) => Ok(Value::from(s)),
            Err(_) => Err("unable to decode value to base64".into()),
        }
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        // Always fallible due to the possibility of decoding errors that VRL can't detect in
        // advance: https://docs.rs/base64/0.13.0/base64/enum.DecodeError.html
        TypeDef::new().bytes().fallible()
    }
}

/*
#[cfg(test)]
mod test {
    use super::*;

    test_type_def![
        valid_charset_fallible {
            expr: |_| DecodeBase64Fn {
                value: lit!("foo").boxed(),
                charset: Some(lit!("standard").boxed()),
            },
            def: TypeDef { fallible: true, kind: Kind::Bytes, ..Default::default() },
        }

        invalid_charset_fallible {
            expr: |_| DecodeBase64Fn {
                value: lit!("foo").boxed(),
                charset: Some(lit!("other").boxed()),
            },
            def: TypeDef { fallible: true, kind: Kind::Bytes, ..Default::default() },
        }

        value_non_string_fallible {
            expr: |_| DecodeBase64Fn {
                value: Literal::from(127).boxed(),
                charset: None,
            },
            def: TypeDef { fallible: true, kind: Kind::Bytes, ..Default::default() },
        }

        all_types_wrong_fallible {
            expr: |_| DecodeBase64Fn {
                value: Literal::from(127).boxed(),
                charset: Some(lit!(127).boxed()),
            },
            def: TypeDef { fallible: true, kind: Kind::Bytes, ..Default::default() },
        }
    ];

    test_function![
        decode_base64 => DecodeBase64;

        with_defaults {
            args: func_args![value: value!("c29tZSs9c3RyaW5nL3ZhbHVl")],
            want: Ok(value!("some+=string/value")),
        }

        with_standard_charset {
            args: func_args![value: value!("c29tZSs9c3RyaW5nL3ZhbHVl"), charset: value!["standard"]],
            want: Ok(value!("some+=string/value")),
        }

        with_urlsafe_charset {
            args: func_args![value: value!("c29tZSs9c3RyaW5nL3ZhbHVl"), charset: value!("url_safe")],
            want: Ok(value!("some+=string/value")),
        }

        empty_string_standard_charset {
            args: func_args![value: value!(""), charset: value!("standard")],
            want: Ok(value!("")),
        }

        empty_string_urlsafe_charset {
            args: func_args![value: value!(""), charset: value!("url_safe")],
            want: Ok(value!("")),
        }
    ];
}
*/
