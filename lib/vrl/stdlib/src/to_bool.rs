use shared::conversion::Conversion;
use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct ToBool;

impl Function for ToBool {
    fn identifier(&self) -> &'static str {
        "to_bool"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            kind: kind::ANY,
            required: true,
        }]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");

        Ok(Box::new(ToBoolFn { value }))
    }
}

#[derive(Debug)]
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
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        use Value::*;

        let value = self.value.resolve(ctx)?;

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
        
        self.value
            .type_def(state)
            .fallible_unless(Kind::Boolean | Kind::Integer | Kind::Float | Kind::Null)
            .with_constraint(Kind::Boolean)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    vrl::test_type_def![
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
        use crate::map;

        let cases = vec![
            (
                map!["foo": "true"],
                Ok(Value::Boolean(true)),
                ToBoolFn::new(Box::new(Path::from("foo"))),
            ),
            (
                map!["foo": 20],
                Ok(Value::Boolean(true)),
                ToBoolFn::new(Box::new(Path::from("foo"))),
            ),
        ];

        let mut state = state::Program::default();

        for (object, exp, func) in cases {
            let mut object: Value = object.into();
            let got = func
                .resolve(&mut ctx)
                .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));

            assert_eq!(got, exp);
        }
    }
}
