use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct ParseJson;

impl Function for ParseJson {
    fn identifier(&self) -> &'static str {
        "parse_json"
    }

    fn summary(&self) -> &'static str {
        "parse a string to a JSON type"
    }

    fn usage(&self) -> &'static str {
        indoc! {r#"
            Parses the provided `value` as JSON.

            Only JSON types are returned. If you need to convert a `string` into a `timestamp`,
            consider the `parse_timestamp` function.
        "#}
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            kind: kind::BYTES,
            required: true,
        }]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "object",
                source: r#"parse_json!(s'{ "field": "value" }')"#,
                result: Ok(r#"{ "field": "value" }"#),
            },
            Example {
                title: "array",
                source: r#"parse_json!("[true, 0]")"#,
                result: Ok("[true, 0]"),
            },
            Example {
                title: "string",
                source: r#"parse_json!(s'"hello"')"#,
                result: Ok("hello"),
            },
            Example {
                title: "integer",
                source: r#"parse_json!("42")"#,
                result: Ok("42"),
            },
            Example {
                title: "float",
                source: r#"parse_json!("42.13")"#,
                result: Ok("42.13"),
            },
            Example {
                title: "boolean",
                source: r#"parse_json!("false")"#,
                result: Ok("false"),
            },
            Example {
                title: "invalid value",
                source: r#"parse_json!("{ INVALID }")"#,
                result: Err(
                    r#"function call error for "parse_json" at (0:26): unable to parse json: key must be a string at line 1 column 3"#,
                ),
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");

        Ok(Box::new(ParseJsonFn { value }))
    }
}

#[derive(Debug, Clone)]
struct ParseJsonFn {
    value: Box<dyn Expression>,
}

impl Expression for ParseJsonFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let bytes = self.value.resolve(ctx)?.unwrap_bytes();
        let value = serde_json::from_slice::<'_, Value>(&bytes)
            .map_err(|e| format!("unable to parse json: {}", e))?;

        Ok(value)
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new()
            .fallible()
            .bytes()
            .add_boolean()
            .add_integer()
            .add_float()
            .add_null()
            .add_array_mapped::<(), Kind>(map! { (): Kind::all() })
            .add_object::<(), Kind>(map! { (): Kind::all() })
    }
}

/*
#[cfg(test)]
mod tests {
    use super::*;

    test_type_def![value_string {
        expr: |_| ParseJsonFn {
            value: lit!("foo").boxed(),
        },
        def: TypeDef {
            fallible: true,
            kind: Kind::Bytes
                | Kind::Boolean
                | Kind::Integer
                | Kind::Float
                | Kind::Array
                | Kind::Map
                | Kind::Null,
            ..Default::default()
        },
    }];
}
*/
