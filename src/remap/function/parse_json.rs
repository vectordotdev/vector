use remap::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct ParseJson;

impl Function for ParseJson {
    fn identifier(&self) -> &'static str {
        "parse_json"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                accepts: |v| matches!(v, Value::Bytes(_)),
                required: true,
            },
            Parameter {
                keyword: "default",
                accepts: |v| matches!(v, Value::Bytes(_)),
                required: false,
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required_expr("value")?;
        let default = arguments.optional_expr("default")?;

        Ok(Box::new(ParseJsonFn { value, default }))
    }
}

#[derive(Debug, Clone)]
struct ParseJsonFn {
    value: Box<dyn Expression>,
    default: Option<Box<dyn Expression>>,
}

impl ParseJsonFn {
    #[cfg(test)]
    fn new(value: Box<dyn Expression>, default: Option<Value>) -> Self {
        let default = default.map(|v| Box::new(Literal::from(v)) as _);

        Self { value, default }
    }
}

impl Expression for ParseJsonFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let to_json = |value| match value {
            Value::Bytes(bytes) => serde_json::from_slice(&bytes)
                .map(|v: serde_json::Value| {
                    let v: crate::event::Value = v.into();
                    v.into()
                })
                .map_err(|err| format!("unable to parse json {}", err).into()),
            _ => Err(format!(r#"unable to convert value "{}" to json"#, value.kind()).into()),
        };

        super::convert_value_or_default(
            self.value.execute(state, object),
            self.default.as_ref().map(|v| v.execute(state, object)),
            to_json,
        )
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        use value::Kind;

        let default_def = self
            .default
            .as_ref()
            .map(|default| default.type_def(state).fallible_unless(Kind::Bytes));

        self.value
            .type_def(state)
            .fallible_unless(Kind::Bytes)
            .merge_with_default_optional(default_def)
            .into_fallible(true) // JSON parsing errors
            .with_constraint(
                Kind::Bytes
                    | Kind::Boolean
                    | Kind::Integer
                    | Kind::Float
                    | Kind::Array
                    | Kind::Map
                    | Kind::Null,
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map;
    use value::Kind;

    remap::test_type_def![
        value_string {
            expr: |_| ParseJsonFn {
                value: Literal::from("foo").boxed(),
                default: None,
            },
            def: TypeDef {
                fallible: true,
                kind: Kind::Bytes | Kind::Boolean | Kind::Integer | Kind::Float | Kind::Array | Kind::Map | Kind::Null,
                ..Default::default()
            },
        }

        optional_default {
            expr: |_| ParseJsonFn {
                value: Literal::from("foo").boxed(),
                default: Some(Box::new(Noop)),
            },
            def: TypeDef {
                fallible: true,
                kind: Kind::Bytes | Kind::Boolean | Kind::Integer | Kind::Float | Kind::Array | Kind::Map | Kind::Null,
                ..Default::default()
            },
        }

        optional_value {
            expr: |_| ParseJsonFn {
                value: Box::new(Noop),
                default: Some(Literal::from("foo").boxed()),
            },
            def: TypeDef {
                fallible: true,
                kind: Kind::Bytes | Kind::Boolean | Kind::Integer | Kind::Float | Kind::Array | Kind::Map | Kind::Null,
                ..Default::default()
            },
        }

        optional_value_and_default {
            expr: |_| ParseJsonFn {
                value: Box::new(Noop),
                default: Some(Box::new(Noop)),
            },
            def: TypeDef {
                fallible: true,
                optional: true,
                kind: Kind::Bytes | Kind::Boolean | Kind::Integer | Kind::Float | Kind::Array | Kind::Map | Kind::Null,
            },
        }
    ];

    #[test]
    fn parse_json() {
        let cases = vec![
            (
                map!["foo": "42"],
                Ok(42.into()),
                ParseJsonFn::new(Box::new(Path::from("foo")), None),
            ),
            (
                map!["foo": "\"hello\""],
                Ok("hello".into()),
                ParseJsonFn::new(Box::new(Path::from("foo")), None),
            ),
            (
                map!["foo": r#"{"field":"value"}"#],
                Ok(map!["field": "value"].into()),
                ParseJsonFn::new(Box::new(Path::from("foo")), None),
            ),
            (
                map!["foo": r#"{ INVALID }"#],
                Ok(42.into()),
                ParseJsonFn::new(Box::new(Path::from("foo")), Some("42".into())),
            ),
            (
                map!["foo": r#"{ INVALID }"#],
                Err("function call error: unable to parse json key must be a string at line 1 column 3".into()),
                ParseJsonFn::new(Box::new(Path::from("foo")), None),
            ),
            (
                map!["foo": r#"{ INVALID }"#],
                Err("function call error: unable to parse json key must be a string at line 1 column 3".into()),
                ParseJsonFn::new(Box::new(Path::from("foo")), Some("{ INVALID }".into())),
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
