vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
ParseJson;

Function for ParseJson {
    identifier(&self) -> &'static str {
        "parse_json"
    }

    summary(&self) -> &'static str {
        "parse a string to a JSON type"
    }

    usage(&self) -> &'static str {
        indoc! {r#"
            Parses the provided `value` as JSON.

            Only JSON types are returned. If you need to convert a `string` into a `timestamp`,
            consider the `parse_timestamp` function.
        "#}
    }

    parameters(&self) -> &'static [Parameter] {
         {
            keyword: "value",
            kind: kind::BYTES,
            required: true,
        }]
    }

    examples(&self) -> &'static [Example] {
        
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

     compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");

        Ok(Box::new(ParseJsonFn { value }))
    }
}

#[derive(Debug, Clone)]
 ParseJsonFn {
    value: Box<dyn Expression>,
}

 Expression  ParseJsonFn {
     resolve(&self, ctx: &mut Context) -> Resolved {
        let bytes = self.value.resolve(ctx)?.try_bytes()?;
        let value = serde_json::from_slice::<'_, Value>(&bytes)
            .map_err(|e| format!("unable to parse json: {}", e))?;

        Ok(value)
    }

     type_def(&self, _: &state::Compiler) -> TypeDef {
        type_def()
    }
}

 inner_kind() -> Kind {
    Kind::Null
        | Kind::Bytes
        | Kind::Integer
        | Kind::Float
        | Kind::Boolean
        | Kind::Array
        | Kind::Object
}

 type_def() -> TypeDef {
    TypeDef::new()
        .fallible()
        .bytes()
        .add_boolean()
        .add_integer()
        .add_float()
        .add_null()
        .add_array_mapped::<(), Kind>(map! { (): inner_kind() })
        .add_object::<(), Kind>(map! { (): inner_kind() })
}

#[cfg(test)]
 tests {
     super::*;

    test_function![
        parse_json => ParseJson;

        parses {
            args: func_args![ value: r#"{"field": "value"}"# ],
            want: Ok(value!({ field: "value" })),
            tdef: type_def(),
        }

        complex_json {
            args: func_args![ value: r#"{"object": {"string":"value","number":42,"array":["hello","world"],"boolean":false}}"# ],
            want: Ok(value!({ object: {string: "value", number: 42, array: ["hello", "world"], boolean: false} })),
            tdef: type_def(),
        }

        invalid_json_errors {
            args: func_args![ value: r#"{"field": "value"# ],
            want: Err("unable to parse json: EOF while parsing a string at line 1 column 16"),
            tdef: TypeDef::new()
                .fallible()
                .bytes()
                .add_boolean()
                .add_integer()
                .add_float()
                .add_null()
                .add_array_mapped::<(), Kind>(map! { (): inner_kind() })
                .add_object::<(), Kind>(map! { (): inner_kind() }),
        }
    ];
}
