use remap::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct ToString;

impl Function for ToString {
    fn identifier(&self) -> &'static str {
        "to_string"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                accepts: |_| true,
                required: true,
            },
            Parameter {
                keyword: "default",
                accepts: |_| true,
                required: false,
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required_expr("value")?;
        let default = arguments.optional_expr("default")?;

        Ok(Box::new(ToStringFn { value, default }))
    }
}

#[derive(Debug, Clone)]
struct ToStringFn {
    value: Box<dyn Expression>,
    default: Option<Box<dyn Expression>>,
}

impl ToStringFn {
    #[cfg(test)]
    fn new(value: Box<dyn Expression>, default: Option<Value>) -> Self {
        let default = default.map(|v| Box::new(Literal::from(v)) as _);
        Self { value, default }
    }
}

impl Expression for ToStringFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let to_string = |value| match value {
            Value::String(_) => Ok(value),
            _ => Ok(value.as_string_lossy()),
        };

        super::convert_value_or_default(
            self.value.execute(state, object),
            self.default.as_ref().map(|v| v.execute(state, object)),
            to_string,
        )
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.value
            .type_def(state)
            .merge_with_default_optional(
                self.default.as_ref().map(|default| default.type_def(state)),
            )
            .with_constraint(value::Kind::String)
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
            expr: |_| ToStringFn { value: Literal::from(true).boxed(), default: None},
            def: TypeDef { kind: Kind::String, ..Default::default() },
        }

        integer_infallible {
            expr: |_| ToStringFn { value: Literal::from(1).boxed(), default: None},
            def: TypeDef { kind: Kind::String, ..Default::default() },
        }

        float_infallible {
            expr: |_| ToStringFn { value: Literal::from(1.0).boxed(), default: None},
            def: TypeDef { kind: Kind::String, ..Default::default() },
        }

        null_infallible {
            expr: |_| ToStringFn { value: Literal::from(()).boxed(), default: None},
            def: TypeDef { kind: Kind::String, ..Default::default() },
        }

        string_infallible {
            expr: |_| ToStringFn { value: Literal::from("foo").boxed(), default: None},
            def: TypeDef { kind: Kind::String, ..Default::default() },
        }

        map_infallible {
            expr: |_| ToStringFn { value: Literal::from(BTreeMap::new()).boxed(), default: None},
            def: TypeDef { kind: Kind::String, ..Default::default() },
        }

        array_infallible {
            expr: |_| ToStringFn { value: Literal::from(vec![0]).boxed(), default: None},
            def: TypeDef { kind: Kind::String, ..Default::default() },
        }

        timestamp_infallible {
            expr: |_| ToStringFn { value: Literal::from(chrono::Utc::now()).boxed(), default: None},
            def: TypeDef { kind: Kind::String, ..Default::default() },
        }

        fallible_value_without_default {
            expr: |_| ToStringFn { value: Variable::new("foo".to_owned()).boxed(), default: None},
            def: TypeDef {
                fallible: true,
                optional: false,
                kind: Kind::String,
            },
        }

        fallible_value_with_fallible_default {
            expr: |_| ToStringFn {
                value: Variable::new("foo".to_owned()).boxed(),
                default: Some(Variable::new("foo".to_owned()).boxed()),
            },
            def: TypeDef {
                fallible: true,
                optional: false,
                kind: Kind::String,
            },
        }

       fallible_value_with_infallible_default {
            expr: |_| ToStringFn {
                value: Variable::new("foo".to_owned()).boxed(),
                default: Some(Literal::from(true).boxed()),
            },
            def: TypeDef {
                fallible: false,
                optional: false,
                kind: Kind::String,
            },
        }

        infallible_value_with_fallible_default {
            expr: |_| ToStringFn {
                value: Literal::from(true).boxed(),
                default: Some(Variable::new("foo".to_owned()).boxed()),
            },
            def: TypeDef {
                fallible: false,
                optional: false,
                kind: Kind::String,
            },
        }

        infallible_value_with_infallible_default {
            expr: |_| ToStringFn {
                value: Literal::from(true).boxed(),
                default: Some(Literal::from(true).boxed()),
            },
            def: TypeDef {
                fallible: false,
                optional: false,
                kind: Kind::String,
            },
        }
    ];

    #[test]
    fn to_string() {
        let cases = vec![
            (
                map![],
                Err("path error: missing path: foo".into()),
                ToStringFn::new(Box::new(Path::from("foo")), None),
            ),
            (
                map![],
                Ok(Value::from("default")),
                ToStringFn::new(Box::new(Path::from("foo")), Some(Value::from("default"))),
            ),
            (
                map!["foo": 20],
                Ok(Value::from("20")),
                ToStringFn::new(Box::new(Path::from("foo")), None),
            ),
            (
                map!["foo": 20.5],
                Ok(Value::from("20.5")),
                ToStringFn::new(Box::new(Path::from("foo")), None),
            ),
        ];

        let mut state = state::Program::default();

        for (mut object, exp, func) in cases {
            let got = func
                .execute(&mut state, &mut object)
                .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));

            assert_eq!(got, exp);
        }
    }
}
