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
            }
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

        let padding = match &self.padding {
            Some(p) => Some(p.execute(state, object)?.try_boolean()?),
            None => None,
        };

        let config = match padding {
            Some(p) if !p => base64::STANDARD_NO_PAD, // Padding enabled by default
            _ => base64::STANDARD,
        };

        Ok(Value::from(base64::encode_config(value, config)))
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        use value::Kind;

        self.value
            .type_def(state)
            .fallible_unless(Kind::Bytes)
            .with_constraint(Kind::Bytes)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use value::Kind;

    test_type_def![
        value_string_infallible {
            expr: |_| EncodeBase64Fn {
                value: Literal::from("foo").boxed(),
                padding: None,
            },
            def: TypeDef { kind: Kind::Bytes, ..Default::default() },
        }

        value_non_string_infallible {
            expr: |_| EncodeBase64Fn {
                value: Literal::from(127).boxed(),
                padding: Some(Literal::from(true).boxed()),
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
