use crate::util::Base64Charset;
use std::str::FromStr;
use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct EncodeBase64;

impl Function for EncodeBase64 {
    fn identifier(&self) -> &'static str {
        "encode_base64"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::BYTES,
                required: true,
            },
            Parameter {
                keyword: "padding",
                kind: kind::BOOLEAN,
                required: false,
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
        let padding = arguments.optional("padding");
        let charset = arguments.optional("charset");

        Ok(Box::new(EncodeBase64Fn {
            value,
            padding,
            charset,
        }))
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "demo string",
            source: r#"encode_base64("some string value", padding: false, charset: "url_safe")"#,
            result: Ok("c29tZSBzdHJpbmcgdmFsdWU"),
        }]
    }
}

#[derive(Clone, Debug)]
struct EncodeBase64Fn {
    value: Box<dyn Expression>,
    padding: Option<Box<dyn Expression>>,
    charset: Option<Box<dyn Expression>>,
}

impl Expression for EncodeBase64Fn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?.try_bytes()?;

        let padding = self
            .padding
            .as_ref()
            .map(|p| {
                p.resolve(ctx)
                    .and_then(|v| Value::try_boolean(v).map_err(Into::into))
            })
            .transpose()?
            .unwrap_or(true);

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

        let config = base64::Config::new(charset.into(), padding);

        Ok(base64::encode_config(value, config).into())
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().bytes().infallible()
    }
}

/*
#[cfg(test)]
mod test {
    use super::*;

    test_type_def![
        value_string_padding_unspecified_charset_unspecified_infallible {

                value: lit!("foo").boxed(),
                padding: None,
                charset: None,
            },
            def: TypeDef { kind: Kind::Bytes, ..Default::default() },
        }

        valid_charset_fallible {
            expr: |_| EncodeBase64Fn {
                value: lit!("foo").boxed(),
                padding: Some(lit!(false).boxed()),
                charset: Some(lit!("standard").boxed()),
            },
            def: TypeDef { fallible: true, kind: Kind::Bytes, ..Default::default() },
        }

        padding_non_boolean_fallible {
            expr: |_| EncodeBase64Fn {
                value: lit!("foo").boxed(),
                padding: Some(lit!("foo").boxed()),
                charset: None,
            },
            def: TypeDef { fallible: true, kind: Kind::Bytes, ..Default::default() },
        }

        value_non_string_fallible {
            expr: |_| EncodeBase64Fn {
                value: Literal::from(127).boxed(),
                padding: None,
                charset: None,
            },
            def: TypeDef { fallible: true, kind: Kind::Bytes, ..Default::default() },
        }

        all_types_wrong_fallible {
            expr: |_| EncodeBase64Fn {
                value: Literal::from(127).boxed(),
                padding: Some(lit!("foo").boxed()),
                charset: Some(lit!(127).boxed()),
            },
            def: TypeDef { fallible: true, kind: Kind::Bytes, ..Default::default() },
        }
    ];

    test_function![
        encode_base64 => EncodeBase64;

        with_defaults {
            args: func_args![value: value!("some+=string/value")],
            want: Ok(value!("c29tZSs9c3RyaW5nL3ZhbHVl")),
        }

        with_padding_standard_charset {
            args: func_args![value: value!("some+=string/value"), padding: value!(true), charset: value!("standard")],
            want: Ok(value!("c29tZSs9c3RyaW5nL3ZhbHVl")),
        }

        no_padding_standard_charset {
            args: func_args![value: value!("some+=string/value"), padding: value!(false), charset: value!("standard")],
            want: Ok(value!("c29tZSs9c3RyaW5nL3ZhbHVl")),
        }

        with_padding_urlsafe_charset {
            args: func_args![value: value!("some+=string/value"), padding: value!(true), charset: value!("url_safe")],
            want: Ok(value!("c29tZSs9c3RyaW5nL3ZhbHVl")),
        }

        no_padding_urlsafe_charset {
            args: func_args![value: value!("some+=string/value"), padding: value!(false), charset: value!("url_safe")],
            want: Ok(value!("c29tZSs9c3RyaW5nL3ZhbHVl")),
        }

        empty_string_standard_charset {
            args: func_args![value: value!(""), charset: value!("standard")],
            want: Ok(value!("")),
        }

        empty_string_urlsafe_charset {
            args: func_args![value: value!(""), charset: value!("url_safe")],
            want: Ok(value!("")),
        }

        invalid_charset_error {
            args: func_args![value: value!("some string value"), padding: value!(false), charset: value!("foo")],
            want: Err("function call error: unknown charset"),
        }
    ];
}
*/
