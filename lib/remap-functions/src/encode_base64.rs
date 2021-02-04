use crate::util::Base64Charset;
use remap::prelude::*;
use std::str::FromStr;

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
                accepts: |v| matches!(v, Value::Bytes(_)),
                required: true,
            },
            Parameter {
                keyword: "padding",
                accepts: |v| matches!(v, Value::Boolean(_)),
                required: false,
            },
            Parameter {
                keyword: "charset",
                accepts: |v| matches!(v, Value::Bytes(_)),
                required: false,
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required("value")?.boxed();
        let padding = arguments.optional("padding").map(Expr::boxed);
        let charset = arguments.optional("charset").map(Expr::boxed);

        Ok(Box::new(EncodeBase64Fn {
            value,
            padding,
            charset,
        }))
    }
}

#[derive(Clone, Debug)]
struct EncodeBase64Fn {
    value: Box<dyn Expression>,
    padding: Option<Box<dyn Expression>>,
    charset: Option<Box<dyn Expression>>,
}

impl Expression for EncodeBase64Fn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let value = self.value.execute(state, object)?.try_bytes()?;

        let padding = self
            .padding
            .as_ref()
            .map(|p| {
                p.execute(state, object)
                    .and_then(|v| Value::try_boolean(v).map_err(Into::into))
            })
            .transpose()?
            .unwrap_or(true);

        let charset = self
            .charset
            .as_ref()
            .map(|c| {
                c.execute(state, object)
                    .and_then(|v| Value::try_bytes(v).map_err(Into::into))
            })
            .transpose()?
            .map(|c| Base64Charset::from_str(&String::from_utf8_lossy(&c)))
            .transpose()?
            .unwrap_or_default();

        let config = base64::Config::new(charset.into(), padding);

        Ok(base64::encode_config(value, config).into())
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        use value::Kind;

        let padding_def = self
            .padding
            .as_ref()
            .map(|padding| padding.type_def(state).fallible_unless(Kind::Boolean));

        let charset_def = self
            .charset
            .as_ref()
            .map(|charset| charset.type_def(state).into_fallible(true));

        self.value
            .type_def(state)
            .fallible_unless(Kind::Bytes)
            .merge_optional(padding_def)
            .merge_optional(charset_def)
            .with_constraint(Kind::Bytes)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use value::Kind;

    test_type_def![
        value_string_padding_unspecified_charset_unspecified_infallible {
            expr: |_| EncodeBase64Fn {
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
