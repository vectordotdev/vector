use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct DecodeBase64;

impl Function for DecodeBase64 {
    fn identifier(&self) -> &'static str {
        "decode_base64"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            kind: kind::BYTES,
            required: true,
        }]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");

        Ok(Box::new(DecodeBase64Fn { value }))
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "demo string",
                source: r#"decode_base64!("c29tZSBzdHJpbmcgdmFsdWU=")"#,
                result: Ok(r#"some string value"#)
            }
        ]
    }
}

#[derive(Clone, Debug)]
struct DecodeBase64Fn {
    value: Box<dyn Expression>,
}

impl Expression for DecodeBase64Fn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?.try_bytes()?;

        base64::decode(value)
            .map(Into::into)
            .map_err(|_| "unable to decode value to base64".into())
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
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
        value_string_fallible {
            expr: |_| DecodeBase64Fn {
                value: Literal::from("foo").boxed(),
            },
            def: TypeDef { fallible: true, kind: Kind::Bytes, ..Default::default() },
        }

        value_non_string_fallible {
            expr: |_| DecodeBase64Fn {
                value: Literal::from(127).boxed(),
            },
            def: TypeDef { fallible: true, kind: Kind::Bytes, ..Default::default() },
        }
    ];

    test_function![
        decode_base64 => DecodeBase64;

        string_value_with_padding {
            args: func_args![value: value!("c29tZSBzdHJpbmcgdmFsdWU=")],
            want: Ok(value!("some string value")),
        }

        string_value_no_padding {
            args: func_args![value: value!("c29tZSBzdHJpbmcgdmFsdWU")],
            want: Ok(value!("some string value")),
        }

        empty_string_value {
            args: func_args![value: value!("")],
            want: Ok(value!("")),
        }
    ];
}
*/
