use remap::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct EncodeBase64;

impl Function for EncodeBase64 {
    fn identifier(&self) -> &'static str {
        "encode_base64"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter{
                keyword: "value",
                accepts: |v| matches!(v, Value::Bytes(_)),
                required: true,
            }
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required("value")?.boxed();

        Ok(Box::new(EncodeBase64Fn { value }))
    }
}

#[derive(Clone, Debug)]
struct EncodeBase64Fn {
    value: Box<dyn Expression>,
}

impl Expression for EncodeBase64Fn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let value = self.value.execute(state, object)?.try_bytes()?;

        Ok(Value::from(base64::encode(value)))
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
            },
            def: TypeDef { kind: Kind::Bytes, ..Default::default() },
        }

        value_non_string_infallible {
            expr: |_| EncodeBase64Fn {
                value: Literal::from(127).boxed(),
            },
            def: TypeDef { fallible: true, kind: Kind::Bytes, ..Default::default() },
        }
    ];

    test_function![
        encode_base64 => EncodeBase64;

        string_value {
            args: func_args![value: value!("some string value")],
            want: Ok(value!("c29tZSBzdHJpbmcgdmFsdWU=")),
        }
    ];
}
