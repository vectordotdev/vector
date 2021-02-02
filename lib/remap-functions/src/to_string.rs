use remap::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct ToString;

impl Function for ToString {
    fn identifier(&self) -> &'static str {
        "to_string"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            accepts: |_| true,
            required: true,
        }]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required("value")?.boxed();

        Ok(Box::new(ToStringFn { value }))
    }
}

#[derive(Debug, Clone)]
struct ToStringFn {
    value: Box<dyn Expression>,
}

impl ToStringFn {
    #[cfg(test)]
    fn new(value: Box<dyn Expression>) -> Self {
        Self { value }
    }
}

impl Expression for ToStringFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        use Value::*;

        let value = self.value.execute(state, object)?;

        match value {
            Bytes(_) => Ok(value),
            Integer(v) => Ok(v.to_string().into()),
            Float(v) => Ok(v.to_string().into()),
            Boolean(v) => Ok(v.to_string().into()),
            Timestamp(v) => Ok(v.to_string().into()),
            Regex(v) => Ok(v.to_string().into()),
            Null => Ok("".into()),
            Map(_) | Array(_) => Err("unable to convert value to string".into()),
        }
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        use value::Kind;

        self.value
            .type_def(state)
            .fallible_unless(
                Kind::Bytes
                    | Kind::Integer
                    | Kind::Float
                    | Kind::Boolean
                    | Kind::Timestamp
                    | Kind::Regex
                    | Kind::Null,
            )
            .with_constraint(value::Kind::Bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use value::Kind;

    remap::test_type_def![
        boolean_infallible {
            expr: |_| ToStringFn { value: lit!(true).boxed() },
            def: TypeDef { kind: Kind::Bytes, ..Default::default() },
        }

        integer_infallible {
            expr: |_| ToStringFn { value: lit!(1).boxed() },
            def: TypeDef { kind: Kind::Bytes, ..Default::default() },
        }

        float_infallible {
            expr: |_| ToStringFn { value: lit!(1.0).boxed() },
            def: TypeDef { kind: Kind::Bytes, ..Default::default() },
        }

        null_infallible {
            expr: |_| ToStringFn { value: lit!(null).boxed() },
            def: TypeDef { kind: Kind::Bytes, ..Default::default() },
        }

        string_infallible {
            expr: |_| ToStringFn { value: lit!("foo").boxed() },
            def: TypeDef { kind: Kind::Bytes, ..Default::default() },
        }

        timestamp_infallible {
            expr: |_| ToStringFn { value: Literal::from(chrono::Utc::now()).boxed() },
            def: TypeDef { kind: Kind::Bytes, ..Default::default() },
        }

        map_fallible {
            expr: |_| ToStringFn { value: map!{}.boxed() },
            def: TypeDef { fallible: true, kind: Kind::Bytes, ..Default::default() },
        }

        array_fallible {
            expr: |_| ToStringFn { value: array![].boxed() },
            def: TypeDef { fallible: true, kind: Kind::Bytes, ..Default::default() },
        }
    ];

    #[test]
    fn to_string() {
        use shared::btreemap;

        let cases = vec![
            (
                btreemap! { "foo" => 20 },
                Ok(Value::from("20")),
                ToStringFn::new(Box::new(Path::from("foo"))),
            ),
            (
                btreemap! { "foo" => 20.5 },
                Ok(Value::from("20.5")),
                ToStringFn::new(Box::new(Path::from("foo"))),
            ),
        ];

        let mut state = state::Program::default();

        for (object, exp, func) in cases {
            let mut object: Value = object.into();
            let got = func
                .execute(&mut state, &mut object)
                .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));

            assert_eq!(got, exp);
        }
    }
}
