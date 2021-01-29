use remap::prelude::*;
use shared::conversion::Conversion;

#[derive(Clone, Copy, Debug)]
pub struct ToBool;

impl Function for ToBool {
    fn identifier(&self) -> &'static str {
        "to_bool"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            accepts: crate::util::is_scalar_value,
            required: true,
        }]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required("value")?.boxed();

        Ok(Box::new(ToBoolFn { value }))
    }
}

#[derive(Debug, Clone)]
struct ToBoolFn {
    value: Box<dyn Expression>,
}

impl ToBoolFn {
    #[cfg(test)]
    fn new(value: Box<dyn Expression>) -> Self {
        Self { value }
    }
}

impl Expression for ToBoolFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        use Value::*;

        let value = self.value.execute(state, object)?;

        match value {
            Boolean(_) => Ok(value),
            Integer(v) => Ok(Boolean(v != 0)),
            Float(v) => Ok(Boolean(v != 0.0)),
            Null => Ok(Boolean(false)),
            Bytes(v) => Conversion::Boolean
                .convert(v)
                .map_err(|e| e.to_string().into()),
            Array(_) | Map(_) | Timestamp(_) | Regex(_) => {
                Err("unable to convert value to boolean".into())
            }
        }
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        use value::Kind;

        self.value
            .type_def(state)
            .fallible_unless(Kind::Boolean | Kind::Integer | Kind::Float | Kind::Null)
            .with_constraint(Kind::Boolean)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use value::Kind;

    remap::test_type_def![
        boolean_infallible {
            expr: |_| ToBoolFn { value: lit!(true).boxed() },
            def: TypeDef { kind: Kind::Boolean, ..Default::default() },
        }

        integer_infallible {
            expr: |_| ToBoolFn { value: lit!(1).boxed() },
            def: TypeDef { kind: Kind::Boolean, ..Default::default() },
        }

        float_infallible {
            expr: |_| ToBoolFn { value: lit!(1.0).boxed() },
            def: TypeDef { kind: Kind::Boolean, ..Default::default() },
        }

        null_infallible {
            expr: |_| ToBoolFn { value: lit!(null).boxed() },
            def: TypeDef { kind: Kind::Boolean, ..Default::default() },
        }

        string_fallible {
            expr: |_| ToBoolFn { value: lit!("foo").boxed() },
            def: TypeDef { fallible: true, kind: Kind::Boolean, ..Default::default() },
        }

        map_fallible {
            expr: |_| ToBoolFn { value: map!{}.boxed() },
            def: TypeDef { fallible: true, kind: Kind::Boolean, ..Default::default() },
        }

        array_fallible {
            expr: |_| ToBoolFn { value: array![].boxed() },
            def: TypeDef { fallible: true, kind: Kind::Boolean, ..Default::default() },
        }

        timestamp_fallible {
            expr: |_| ToBoolFn { value: Literal::from(chrono::Utc::now()).boxed() },
            def: TypeDef { fallible: true, kind: Kind::Boolean, ..Default::default() },
        }

        fallible_value_without_default {
            expr: |_| ToBoolFn { value: lit!("foo").boxed() },
            def: TypeDef {
                fallible: true,
                kind: Kind::Boolean,
                ..Default::default()
            },
        }
    ];

    #[test]
    fn to_bool() {
        use shared::btreemap;

        let cases = vec![
            (
                btreemap! { "foo" => "true" },
                Ok(Value::Boolean(true)),
                ToBoolFn::new(Box::new(Path::from("foo"))),
            ),
            (
                btreemap! { "foo" => 20 },
                Ok(Value::Boolean(true)),
                ToBoolFn::new(Box::new(Path::from("foo"))),
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
