use remap::prelude::*;
use shared::conversion::Conversion;

#[derive(Clone, Copy, Debug)]
pub struct ToInt;

impl Function for ToInt {
    fn identifier(&self) -> &'static str {
        "to_int"
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

        Ok(Box::new(ToIntFn { value, default }))
    }
}

#[derive(Debug, Clone)]
struct ToIntFn {
    value: Box<dyn Expression>,
    default: Option<Box<dyn Expression>>,
}

impl ToIntFn {
    #[cfg(test)]
    fn new(value: Box<dyn Expression>, default: Option<Value>) -> Self {
        let default = default.map(|v| Box::new(Literal::from(v)) as _);
        Self { value, default }
    }
}

impl Expression for ToIntFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        use Value::*;

        let to_int = |value| match value {
            Integer(_) => Ok(value),
            Float(v) => Ok(Integer(v as i64)),
            Boolean(v) => Ok(Integer(if v { 1 } else { 0 })),
            Null => Ok(0.into()),
            Bytes(v) => Conversion::Integer
                .convert(v)
                .map_err(|e| e.to_string().into()),
            Array(_) | Map(_) | Timestamp(_) | Regex(_) => {
                Err("unable to convert value to integer".into())
            }
        };

        crate::util::convert_value_or_default(
            self.value.execute(state, object),
            self.default.as_ref().map(|v| v.execute(state, object)),
            to_int,
        )
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        use value::Kind;

        self.value
            .type_def(state)
            .fallible_unless(Kind::Integer | Kind::Float | Kind::Boolean | Kind::Null)
            .merge_with_default_optional(self.default.as_ref().map(|default| {
                default
                    .type_def(state)
                    .fallible_unless(Kind::Integer | Kind::Float | Kind::Boolean | Kind::Null)
            }))
            .with_constraint(Kind::Integer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use value::Kind;

    remap::test_type_def![
        boolean_infallible {
            expr: |_| ToIntFn { value: Literal::from(true).boxed(), default: None },
            def: TypeDef { kind: Kind::Integer, ..Default::default() },
        }

        integer_infallible {
            expr: |_| ToIntFn { value: Literal::from(1).boxed(), default: None },
            def: TypeDef { kind: Kind::Integer, ..Default::default() },
        }

        float_infallible {
            expr: |_| ToIntFn { value: Literal::from(1.0).boxed(), default: None },
            def: TypeDef { kind: Kind::Integer, ..Default::default() },
        }

        null_infallible {
            expr: |_| ToIntFn { value: Literal::from(()).boxed(), default: None },
            def: TypeDef { kind: Kind::Integer, ..Default::default() },
        }

        string_fallible {
            expr: |_| ToIntFn { value: Literal::from("foo").boxed(), default: None },
            def: TypeDef { fallible: true, kind: Kind::Integer, ..Default::default() },
        }

        map_fallible {
            expr: |_| ToIntFn { value: map!{}.boxed(), default: None },
            def: TypeDef { fallible: true, kind: Kind::Integer, ..Default::default() },
        }

        array_fallible {
            expr: |_| ToIntFn { value: array![].boxed(), default: None },
            def: TypeDef { fallible: true, kind: Kind::Integer, ..Default::default() },
        }

        timestamp_infallible {
            expr: |_| ToIntFn { value: Literal::from(chrono::Utc::now()).boxed(), default: None },
            def: TypeDef { fallible: true, kind: Kind::Integer, ..Default::default() },
        }

        fallible_value_without_default {
            expr: |_| ToIntFn { value: Variable::new("foo".to_owned(), None).boxed(), default: None },
            def: TypeDef {
                fallible: true,
                kind: Kind::Integer,
                ..Default::default()
            },
        }

       fallible_value_with_fallible_default {
            expr: |_| ToIntFn {
                value: array![].boxed(),
                default: Some(array![].boxed()),
            },
            def: TypeDef {
                fallible: true,
                kind: Kind::Integer,
                ..Default::default()
            },
        }

       fallible_value_with_infallible_default {
            expr: |_| ToIntFn {
                value: array![].boxed(),
                default: Some(Literal::from(1).boxed()),
            },
            def: TypeDef {
                fallible: false,
                kind: Kind::Integer,
                ..Default::default()
            },
        }

        infallible_value_with_fallible_default {
            expr: |_| ToIntFn {
                value: Literal::from(1).boxed(),
                default: Some(array![].boxed()),
            },
            def: TypeDef {
                fallible: false,
                kind: Kind::Integer,
                ..Default::default()
            },
        }

        infallible_value_with_infallible_default {
            expr: |_| ToIntFn {
                value: Literal::from(1).boxed(),
                default: Some(Literal::from(1).boxed()),
            },
            def: TypeDef {
                fallible: false,
                kind: Kind::Integer,
                ..Default::default()
            },
        }
    ];

    #[test]
    fn to_int() {
        use crate::map;

        let cases = vec![
            (
                map![],
                Ok(Value::Integer(10)),
                ToIntFn::new(Array::from(vec![0]).boxed(), Some(10.into())),
            ),
            (
                map!["foo": "20"],
                Ok(Value::Integer(20)),
                ToIntFn::new(Box::new(Path::from("foo")), None),
            ),
            (
                map!["foo": 20.5],
                Ok(Value::Integer(20)),
                ToIntFn::new(Box::new(Path::from("foo")), None),
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
