use remap::prelude::*;
use shared::conversion::Conversion;

#[derive(Clone, Copy, Debug)]
pub struct ToFloat;

impl Function for ToFloat {
    fn identifier(&self) -> &'static str {
        "to_float"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                accepts: crate::util::is_scalar_value,
                required: true,
            },
            Parameter {
                keyword: "default",
                accepts: crate::util::is_scalar_value,
                required: false,
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required("value")?.boxed();
        let default = arguments.optional("default").map(Expr::boxed);

        Ok(Box::new(ToFloatFn { value, default }))
    }
}

#[derive(Debug, Clone)]
struct ToFloatFn {
    value: Box<dyn Expression>,
    default: Option<Box<dyn Expression>>,
}

impl ToFloatFn {
    #[cfg(test)]
    fn new(value: Box<dyn Expression>, default: Option<Value>) -> Self {
        let default = default.map(|v| Box::new(Literal::from(v)) as _);
        Self { value, default }
    }
}

impl Expression for ToFloatFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        use Value::*;

        let to_float = |value| match value {
            Float(_) => Ok(value),
            Integer(v) => Ok(Float(v as f64)),
            Boolean(v) => Ok(Float(if v { 1.0 } else { 0.0 })),
            Null => Ok(0.0.into()),
            Bytes(v) => Conversion::Float
                .convert(v)
                .map_err(|e| e.to_string().into()),
            Array(_) | Map(_) | Timestamp(_) | Regex(_) => {
                Err("unable to convert value to float".into())
            }
        };

        crate::util::convert_value_or_default(
            self.value.execute(state, object),
            self.default.as_ref().map(|v| v.execute(state, object)),
            to_float,
        )
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        use value::Kind;

        self.value
            .type_def(state)
            .fallible_unless(Kind::Float | Kind::Integer | Kind::Boolean | Kind::Null)
            .merge_with_default_optional(self.default.as_ref().map(|default| {
                default
                    .type_def(state)
                    .fallible_unless(Kind::Float | Kind::Integer | Kind::Boolean | Kind::Null)
            }))
            .with_constraint(Kind::Float)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use value::Kind;

    remap::test_type_def![
        boolean_infallible {
            expr: |_| ToFloatFn { value: Literal::from(true).boxed(), default: None },
            def: TypeDef { kind: Kind::Float, ..Default::default() },
        }

        integer_infallible {
            expr: |_| ToFloatFn { value: Literal::from(1).boxed(), default: None },
            def: TypeDef { kind: Kind::Float, ..Default::default() },
        }

        float_infallible {
            expr: |_| ToFloatFn { value: Literal::from(1.0).boxed(), default: None },
            def: TypeDef { kind: Kind::Float, ..Default::default() },
        }

        null_infallible {
            expr: |_| ToFloatFn { value: Literal::from(()).boxed(), default: None },
            def: TypeDef { kind: Kind::Float, ..Default::default() },
        }

        string_fallible {
            expr: |_| ToFloatFn { value: Literal::from("foo").boxed(), default: None },
            def: TypeDef { fallible: true, kind: Kind::Float, ..Default::default() },
        }

        map_fallible {
            expr: |_| ToFloatFn { value: map!{}.boxed(), default: None },
            def: TypeDef { fallible: true, kind: Kind::Float, ..Default::default() },
        }

        array_fallible {
            expr: |_| ToFloatFn { value: array![].boxed(), default: None },
            def: TypeDef { fallible: true, kind: Kind::Float, ..Default::default() },
        }

        timestamp_infallible {
            expr: |_| ToFloatFn { value: Literal::from(chrono::Utc::now()).boxed(), default: None },
            def: TypeDef { fallible: true, kind: Kind::Float, ..Default::default() },
        }

        fallible_value_without_default {
            expr: |_| ToFloatFn { value: Variable::new("foo".to_owned(), None).boxed(), default: None },
            def: TypeDef {
                fallible: true,
                kind: Kind::Float,
                ..Default::default()
            },
        }

       fallible_value_with_fallible_default {
            expr: |_| ToFloatFn {
                value: array![].boxed(),
                default: Some(array![].boxed()),
            },
            def: TypeDef {
                fallible: true,
                kind: Kind::Float,
                ..Default::default()
            },
        }

       fallible_value_with_infallible_default {
            expr: |_| ToFloatFn {
                value: array![].boxed(),
                default: Some(Literal::from(1).boxed()),
            },
            def: TypeDef {
                fallible: false,
                kind: Kind::Float,
                ..Default::default()
            },
        }

        infallible_value_with_fallible_default {
            expr: |_| ToFloatFn {
                value: Literal::from(1).boxed(),
                default: Some(array![].boxed()),
            },
            def: TypeDef {
                fallible: false,
                kind: Kind::Float,
                ..Default::default()
            },
        }

        infallible_value_with_infallible_default {
            expr: |_| ToFloatFn {
                value: Literal::from(1).boxed(),
                default: Some(Literal::from(1).boxed()),
            },
            def: TypeDef {
                fallible: false,
                kind: Kind::Float,
                ..Default::default()
            },
        }
    ];

    #[test]
    fn to_float() {
        let cases = vec![
            (
                Ok(Value::Float(10.0)),
                ToFloatFn::new(array![0].boxed(), Some(10.0.into())),
            ),
            (
                Ok(Value::Float(20.5)),
                ToFloatFn::new(Literal::from(value!(20.5)).boxed(), None),
            ),
            (
                Ok(Value::Float(20.0)),
                ToFloatFn::new(Literal::from(value!(20)).boxed(), None),
            ),
        ];

        let mut state = state::Program::default();

        for (exp, func) in cases {
            let got = func
                .execute(&mut state, &mut value!({}))
                .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));

            assert_eq!(got, exp);
        }
    }
}
