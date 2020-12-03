use crate::types::Conversion;
use remap::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct ToBool;

impl Function for ToBool {
    fn identifier(&self) -> &'static str {
        "to_bool"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                accepts: super::is_scalar_value,
                required: true,
            },
            Parameter {
                keyword: "default",
                accepts: super::is_scalar_value,
                required: false,
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required_expr("value")?;
        let default = arguments.optional_expr("default")?;

        Ok(Box::new(ToBoolFn { value, default }))
    }
}

#[derive(Debug, Clone)]
struct ToBoolFn {
    value: Box<dyn Expression>,
    default: Option<Box<dyn Expression>>,
}

impl ToBoolFn {
    #[cfg(test)]
    fn new(value: Box<dyn Expression>, default: Option<Value>) -> Self {
        let default = default.map(|v| Box::new(Literal::from(v)) as _);
        Self { value, default }
    }
}

impl Expression for ToBoolFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        use Value::*;

        let to_bool = |value| match value {
            Boolean(_) => Ok(value),
            Integer(v) => Ok(Boolean(v != 0)),
            Float(v) => Ok(Boolean(v != 0.0)),
            Null => Ok(Boolean(false)),
            Bytes(_) => Conversion::Boolean
                .convert(value.into())
                .map(Into::into)
                .map_err(|e| e.to_string().into()),
            Array(_) | Map(_) | Timestamp(_) => Err("unable to convert value to boolean".into()),
        };

        super::convert_value_or_default(
            self.value.execute(state, object),
            self.default.as_ref().map(|v| v.execute(state, object)),
            to_bool,
        )
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        use value::Kind;

        self.value
            .type_def(state)
            .fallible_unless(Kind::Boolean | Kind::Integer | Kind::Float | Kind::Null)
            .merge_with_default_optional(self.default.as_ref().map(|default| {
                default
                    .type_def(state)
                    .fallible_unless(Kind::Boolean | Kind::Integer | Kind::Float | Kind::Null)
            }))
            .with_constraint(Kind::Boolean)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map;
    use std::collections::BTreeMap;
    use value::Kind;

    remap::test_type_def![
        boolean_infallible {
            expr: |_| ToBoolFn { value: Literal::from(true).boxed(), default: None },
            def: TypeDef { kind: Kind::Boolean, ..Default::default() },
        }

        integer_infallible {
            expr: |_| ToBoolFn { value: Literal::from(1).boxed(), default: None },
            def: TypeDef { kind: Kind::Boolean, ..Default::default() },
        }

        float_infallible {
            expr: |_| ToBoolFn { value: Literal::from(1.0).boxed(), default: None },
            def: TypeDef { kind: Kind::Boolean, ..Default::default() },
        }

        null_infallible {
            expr: |_| ToBoolFn { value: Literal::from(()).boxed(), default: None },
            def: TypeDef { kind: Kind::Boolean, ..Default::default() },
        }

        string_fallible {
            expr: |_| ToBoolFn { value: Literal::from("foo").boxed(), default: None },
            def: TypeDef { fallible: true, kind: Kind::Boolean },
        }

        map_fallible {
            expr: |_| ToBoolFn { value: Literal::from(BTreeMap::new()).boxed(), default: None },
            def: TypeDef { fallible: true, kind: Kind::Boolean },
        }

        array_fallible {
            expr: |_| ToBoolFn { value: Literal::from(vec![0]).boxed(), default: None },
            def: TypeDef { fallible: true, kind: Kind::Boolean },
        }

        timestamp_fallible {
            expr: |_| ToBoolFn { value: Literal::from(chrono::Utc::now()).boxed(), default: None },
            def: TypeDef { fallible: true, kind: Kind::Boolean },
        }

        fallible_value_without_default {
            expr: |_| ToBoolFn { value: Literal::from("foo".to_owned()).boxed(), default: None },
            def: TypeDef {
                fallible: true,
                kind: Kind::Boolean,
            },
        }

       fallible_value_with_fallible_default {
            expr: |_| ToBoolFn {
                value: Literal::from(vec![0]).boxed(),
                default: Some(Literal::from(vec![0]).boxed()),
            },
            def: TypeDef {
                fallible: true,
                kind: Kind::Boolean,
            },
        }

       fallible_value_with_infallible_default {
            expr: |_| ToBoolFn {
                value: Literal::from(vec![0]).boxed(),
                default: Some(Literal::from(true).boxed()),
            },
            def: TypeDef {
                fallible: false,
                kind: Kind::Boolean,
            },
        }

        infallible_value_with_fallible_default {
            expr: |_| ToBoolFn {
                value: Literal::from(true).boxed(),
                default: Some(Literal::from(vec![0]).boxed()),
            },
            def: TypeDef {
                fallible: false,
                kind: Kind::Boolean,
            },
        }

        infallible_value_with_infallible_default {
            expr: |_| ToBoolFn {
                value: Literal::from(true).boxed(),
                default: Some(Literal::from(true).boxed()),
            },
            def: TypeDef {
                fallible: false,
                kind: Kind::Boolean,
            },
        }
    ];

    #[test]
    fn to_bool() {
        let cases = vec![
            (
                map![],
                Ok(Value::Boolean(true)),
                ToBoolFn::new(Literal::from(vec![0]).boxed(), Some(true.into())),
            ),
            (
                map!["foo": "true"],
                Ok(Value::Boolean(true)),
                ToBoolFn::new(Box::new(Path::from("foo")), None),
            ),
            (
                map!["foo": 20],
                Ok(Value::Boolean(true)),
                ToBoolFn::new(Box::new(Path::from("foo")), None),
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
