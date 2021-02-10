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

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "integer (0)",
                source: "to_bool(0)",
                result: Ok("false"),
            },
            Example {
                title: "integer (other)",
                source: "to_bool(2)",
                result: Ok("true"),
            },
            Example {
                title: "float (0)",
                source: "to_bool(0.0)",
                result: Ok("false"),
            },
            Example {
                title: "float (other)",
                source: "to_bool(5.6)",
                result: Ok("true"),
            },
            Example {
                title: "true",
                source: "to_bool(true)",
                result: Ok("true"),
            },
            Example {
                title: "false",
                source: "to_bool(false)",
                result: Ok("false"),
            },
            Example {
                title: "null",
                source: "to_bool(null)",
                result: Ok("0"),
            },
            Example {
                title: "timestamp",
                source: "to_bool(t'2020-01-01T00:00:00Z')",
                result: Ok("1577836800"),
            },
            Example {
                title: "valid string",
                source: "to_bool!(s'5')",
                result: Ok("5"),
            },
            Example {
                title: "invalid string",
                source: "to_bool!(s'foobar')",
                result: Err(
                    r#"function call error for "to_bool" at (0:18): Invalid integer "foobar": invalid digit found in string"#,
                ),
            },
            Example {
                title: "array",
                source: "to_bool!([])",
                result: Err(
                    r#"function call error for "to_bool" at (0:11): unable to coerce "array" into "integer""#,
                ),
            },
            Example {
                title: "object",
                source: "to_bool!({})",
                result: Err(
                    r#"function call error for "to_bool" at (0:11): unable to coerce "object" into "integer""#,
                ),
            },
            Example {
                title: "regex",
                source: "to_bool!(r'foo')",
                result: Err(
                    r#"function call error for "to_bool" at (0:15): unable to coerce "regex" into "integer""#,
                ),
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");

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
            Array(_) | Object(_) | Timestamp(_) | Regex(_) => {
                Err(format!("unable to convert {} to boolean", value.kind()).into())
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
