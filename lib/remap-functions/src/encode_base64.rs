use remap::prelude::*;

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
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required("value")?.boxed();
        let padding = arguments.optional("padding").map(Expr::boxed);

        Ok(Box::new(EncodeBase64Fn { value, padding }))
    }
}

#[derive(Clone, Debug)]
struct EncodeBase64Fn {
    value: Box<dyn Expression>,
    padding: Option<Box<dyn Expression>>,
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

        let config = if padding {
            base64::STANDARD
        } else {
            base64::STANDARD_NO_PAD
        };

        Ok(base64::encode_config(value, config).into())
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        use value::Kind;

        let padding_def = self
            .padding
            .as_ref()
            .map(|padding| padding.type_def(state).fallible_unless(Kind::Boolean));

        self.value
            .type_def(state)
            .fallible_unless(Kind::Bytes)
            .merge_optional(padding_def)
            .with_constraint(Kind::Bytes)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use value::Kind;

    test_type_def![
        value_string_padding_unspecified_infallible {
            expr: |_| EncodeBase64Fn {
                value: Literal::from("foo").boxed(),
                padding: None,
            },
            def: TypeDef { kind: Kind::Bytes, ..Default::default() },
        }

        value_string_padding_boolean_infallible {
            expr: |_| EncodeBase64Fn {
                value: Literal::from("foo").boxed(),
                padding: Some(Literal::from(false).boxed()),
            },
            def: TypeDef { kind: Kind::Bytes, ..Default::default() },
        }

        value_string_padding_non_boolean_fallible {
            expr: |_| EncodeBase64Fn {
                value: Literal::from("foo").boxed(),
                padding: Some(Literal::from("foo").boxed()),
            },
            def: TypeDef { fallible: true, kind: Kind::Bytes, ..Default::default() },
        }

        value_non_string_fallible {
            expr: |_| EncodeBase64Fn {
                value: Literal::from(127).boxed(),
                padding: Some(Literal::from(true).boxed()),
            },
            def: TypeDef { fallible: true, kind: Kind::Bytes, ..Default::default() },
        }

        both_types_wrong_fallible {
            expr: |_| EncodeBase64Fn {
                value: Literal::from(127).boxed(),
                padding: Some(Literal::from("foo").boxed()),
            },
            def: TypeDef { fallible: true, kind: Kind::Bytes, ..Default::default() },
        }
    ];

    test_function![
        encode_base64 => EncodeBase64;

        string_value_with_padding {
            args: func_args![value: value!("some string value"), padding: value!(true)],
            want: Ok(value!("c29tZSBzdHJpbmcgdmFsdWU=")),
        }

        string_value_with_default_padding {
            args: func_args![value: value!("some string value")],
            want: Ok(value!("c29tZSBzdHJpbmcgdmFsdWU=")),
        }

        string_value_no_padding {
            args: func_args![value: value!("some string value"), padding: value!(false)],
            want: Ok(value!("c29tZSBzdHJpbmcgdmFsdWU")),
        }

        empty_string_value {
            args: func_args![value: value!("")],
            want: Ok(value!("")),
        }
    ];
}
