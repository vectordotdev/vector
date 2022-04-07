use std::str::FromStr;

use vrl::prelude::*;

use crate::util::Base64Charset;

fn encode_base64(value: Value, padding: Option<Value>, charset: Option<Value>) -> Resolved {
    let value = value.try_bytes()?;
    let padding = padding
        .map(|v| v.try_boolean())
        .transpose()?
        .unwrap_or(true);
    let charset = charset
        .map(|v| v.try_bytes())
        .transpose()?
        .map(|c| Base64Charset::from_str(&String::from_utf8_lossy(&c)))
        .transpose()?
        .unwrap_or_default();
    let config = base64::Config::new(charset.into(), padding);

    Ok(base64::encode_config(value, config).into())
}

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

    fn compile(
        &self,
        _state: (&mut state::LocalEnv, &mut state::ExternalEnv),
        _ctx: &mut FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
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

    fn call_by_vm(&self, _ctx: &mut Context, args: &mut VmArgumentList) -> Resolved {
        let value = args.required("value");
        let padding = args.optional("padding");
        let charset = args.optional("charset");

        encode_base64(value, padding, charset)
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
        let value = self.value.resolve(ctx)?;
        let padding = self.padding.as_ref().map(|p| p.resolve(ctx)).transpose()?;
        let charset = self.charset.as_ref().map(|c| c.resolve(ctx)).transpose()?;

        encode_base64(value, padding, charset)
    }

    fn type_def(&self, _: (&state::LocalEnv, &state::ExternalEnv)) -> TypeDef {
        TypeDef::bytes().infallible()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    test_function![
        encode_base64 => EncodeBase64;

        with_defaults {
            args: func_args![value: value!("some+=string/value")],
            want: Ok(value!("c29tZSs9c3RyaW5nL3ZhbHVl")),
            tdef: TypeDef::bytes().infallible(),
        }

        with_padding_standard_charset {
            args: func_args![value: value!("some+=string/value"), padding: value!(true), charset: value!("standard")],
            want: Ok(value!("c29tZSs9c3RyaW5nL3ZhbHVl")),
            tdef: TypeDef::bytes().infallible(),
        }

        no_padding_standard_charset {
            args: func_args![value: value!("some+=string/value"), padding: value!(false), charset: value!("standard")],
            want: Ok(value!("c29tZSs9c3RyaW5nL3ZhbHVl")),
            tdef: TypeDef::bytes().infallible(),
        }

        with_padding_urlsafe_charset {
            args: func_args![value: value!("some+=string/value"), padding: value!(true), charset: value!("url_safe")],
            want: Ok(value!("c29tZSs9c3RyaW5nL3ZhbHVl")),
            tdef: TypeDef::bytes().infallible(),
        }

        no_padding_urlsafe_charset {
            args: func_args![value: value!("some+=string/value"), padding: value!(false), charset: value!("url_safe")],
            want: Ok(value!("c29tZSs9c3RyaW5nL3ZhbHVl")),
            tdef: TypeDef::bytes().infallible(),
        }

        empty_string_standard_charset {
            args: func_args![value: value!(""), charset: value!("standard")],
            want: Ok(value!("")),
            tdef: TypeDef::bytes().infallible(),
        }

        empty_string_urlsafe_charset {
            args: func_args![value: value!(""), charset: value!("url_safe")],
            want: Ok(value!("")),
            tdef: TypeDef::bytes().infallible(),
        }

        invalid_charset_error {
            args: func_args![value: value!("some string value"), padding: value!(false), charset: value!("foo")],
            want: Err("unknown charset"),
            tdef: TypeDef::bytes().infallible(),
        }
    ];
}
