use remap::prelude::*;
use shared::conversion::Conversion;

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
            Bytes(v) => Conversion::Boolean
                .convert(v)
                .map_err(|e| e.to_string().into()),
            Array(_) | Map(_) | Timestamp(_) | Regex(_) => {
                Err("unable to convert value to boolean".into())
            }
        };

        crate::util::convert_value_or_default(
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
            def: TypeDef { fallible: true, kind: Kind::Boolean, ..Default::default() },
        }

        map_fallible {
            expr: |_| ToBoolFn { value: map!{}.boxed(), default: None },
            def: TypeDef { fallible: true, kind: Kind::Boolean, ..Default::default() },
        }

        array_fallible {
            expr: |_| ToBoolFn { value: array![].boxed(), default: None },
            def: TypeDef { fallible: true, kind: Kind::Boolean, ..Default::default() },
        }

        timestamp_fallible {
            expr: |_| ToBoolFn { value: Literal::from(chrono::Utc::now()).boxed(), default: None },
            def: TypeDef { fallible: true, kind: Kind::Boolean, ..Default::default() },
        }

        fallible_value_without_default {
            expr: |_| ToBoolFn { value: Literal::from("foo".to_owned()).boxed(), default: None },
            def: TypeDef {
                fallible: true,
                kind: Kind::Boolean,
                ..Default::default()
            },
        }

       fallible_value_with_fallible_default {
            expr: |_| ToBoolFn {
                value: array![].boxed(),
                default: Some(array![].boxed()),
            },
            def: TypeDef {
                fallible: true,
                kind: Kind::Boolean,
                ..Default::default()
            },
        }

       fallible_value_with_infallible_default {
            expr: |_| ToBoolFn {
                value: array![].boxed(),
                default: Some(Literal::from(true).boxed()),
            },
            def: TypeDef {
                fallible: false,
                kind: Kind::Boolean,
                ..Default::default()
            },
        }

        infallible_value_with_fallible_default {
            expr: |_| ToBoolFn {
                value: Literal::from(true).boxed(),
                default: Some(array![].boxed()),
            },
            def: TypeDef {
                fallible: false,
                kind: Kind::Boolean,
                ..Default::default()
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
                ..Default::default()
            },
        }
    ];

    #[test]
    fn to_bool() {
        use crate::map;

        let cases = vec![
            (
                map![],
                Ok(Value::Boolean(true)),
                ToBoolFn::new(Array::from(vec![0]).boxed(), Some(true.into())),
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
