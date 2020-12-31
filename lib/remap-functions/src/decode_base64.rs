use remap::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct DecodeBase64;

impl Function for DecodeBase64 {
    fn identifier(&self) -> &'static str {
        "decode_base64"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            accepts: |v| matches!(v, Value::Bytes(_)),
            required: true,
        }]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required("value")?.boxed();

        Ok(Box::new(DecodeBase64Fn { value }))
    }
}

#[derive(Clone, Debug)]
struct DecodeBase64Fn {
    value: Box<dyn Expression>,
}

impl Expression for DecodeBase64Fn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let value = self.value.execute(state, object)?.try_bytes()?;

        match base64::decode(value) {
            Ok(v) => Ok(Value::from(v)),
            Err(_) => Err("unable to decode value to base64".into()),
        }
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.value
            .type_def(state)
            .fallible_unless(value::Kind::Bytes)
            .with_constraint(value::Kind::Bytes)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use value::Kind;

    test_type_def![
        value_string_infallible {
            expr: |_| DecodeBase64Fn {
                value: Literal::from("foo").boxed(),
            },
            def: TypeDef { kind: Kind::Bytes, ..Default::default() },
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

        string_value {
            args: func_args![value: value!("c29tZSBzdHJpbmcgdmFsdWU=")],
            want: Ok(value!("some string value")),
        }

        empty_string_value {
            args: func_args![value: value!("")],
            want: Ok(value!("")),
        }
    ];
}
