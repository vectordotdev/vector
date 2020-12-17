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
        let value = arguments.required("value")?.boxed();
        let default = arguments.optional("default").map(Expr::boxed);

        Ok(Box::new(ParseJsonFn { value, default }))
    }
}

#[derive(Debug, Clone)]
struct ParseJsonFn {
    value: Box<dyn Expression>,
    default: Option<Box<dyn Expression>>,
}

impl Expression for ParseJsonFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let to_json = |value: Value| {
            let bytes = value.unwrap_bytes();
            let value = serde_json::from_slice::<'_, Value>(&bytes)
                .map_err(|e| format!("unable to parse json: {}", e))?;

            Ok(value)
        };

        crate::util::convert_value_or_default(
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

    test_function![
        parse_json => ParseJson;

        number {
            args: func_args![value: "42"],
            want: Ok(42),
        }

        string {
            args: func_args![value: r#""hello""#],
            want: Ok("hello"),
        }

        json {
            args: func_args![value: r#"{"field":"value"}"#],
            want: Ok(map!["field": "value"]),
        }

        default {
            args: func_args![
                value: r#"{ INVALID }"#,
                default: "42",
            ],
            want: Ok(42),
        }

        invalid_value {
            args: func_args![value: r#"{ INVALID }"#],
            want: Err("function call error: unable to parse json: key must be a string at line 1 column 3"),
        }

        invalid_value_and_default {
            args: func_args![
                value: r#"{ INVALID }"#,
                default: r#"{ INVALID }"#,
            ],
            want: Err("function call error: unable to parse json: key must be a string at line 1 column 3"),
        }
    ];

    test_type_def![
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
                kind: Kind::Bytes | Kind::Boolean | Kind::Integer | Kind::Float | Kind::Array | Kind::Map | Kind::Null,
                ..Default::default()
            },
        }
    ];
}
