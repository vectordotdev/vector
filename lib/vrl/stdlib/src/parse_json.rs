use serde_json::value::{RawValue, Value as JsonValue};
use serde_json::{Error, Map};
use std::collections::HashMap;
use vrl::prelude::*;

fn parse_json(value: Value, max_depth: Option<Value>) -> Resolved {
    let bytes = value.try_bytes()?;

    if let Some(md) = max_depth {
        let parsed_depth = positive_int_depth(md)?;

        let raw_value: Box<RawValue> = serde_json::from_slice::<_>(&bytes)
            .map_err(|e| format!("unable to read json: {}", e))?;

        let res = parse_once_with_depth(raw_value, parsed_depth)
            .map_err(|e| format!("unable to parse json with max depth: {}", e))?;
        Ok(Value::from(res))
    } else {
        let value = serde_json::from_slice::<'_, Value>(&bytes)
            .map_err(|e| format!("unable to parse json: {}", e))?;
        Ok(value)
    }
}

fn positive_int_depth(value: Value) -> std::result::Result<i64, ExpressionError> {
    let res = value.try_integer()?;
    if !(1..=128).contains(&res) {
        Err(ExpressionError::from(format!(
            "max_depth value should be greater than 0 and less than 128, got {}",
            res
        )))
    } else {
        Ok(res)
    }
}

fn parse_once_with_depth(
    value: Box<RawValue>,
    max_depth: i64,
) -> std::result::Result<JsonValue, Error> {
    if value.get().starts_with('{') {
        if max_depth == 0 {
            serde_json::value::to_value(value.to_string())
        } else {
            let map: HashMap<String, Box<RawValue>> = serde_json::from_str(value.get())?;

            let mut res_map: Map<String, JsonValue> = Map::with_capacity(map.len());
            for (k, v) in map {
                res_map.insert(k, parse_once_with_depth(v, max_depth - 1)?);
            }
            Ok(serde_json::Value::from(res_map))
        }
    } else if value.get().starts_with('[') {
        if max_depth == 0 {
            serde_json::value::to_value(value.to_string())
        } else {
            let arr: Vec<Box<RawValue>> = serde_json::from_str(value.get())?;

            let mut res_arr: Vec<JsonValue> = Vec::with_capacity(arr.len());
            for v in arr {
                res_arr.push(parse_once_with_depth(v, max_depth - 1)?)
            }
            Ok(serde_json::Value::from(res_arr))
        }
    } else {
        serde_json::from_str(value.get())
    }
}

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
        &[
            Parameter {
                keyword: "value",
                kind: kind::BYTES,
                required: true,
            },
            Parameter {
                keyword: "max_depth",
                kind: kind::INTEGER,
                required: false,
            },
        ]
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
            Example {
                title: "max_depth",
                source: r#"parse_json!(s'{"first_level":{"second_level":"finish"}}', max_depth: 1)"#,
                result: Ok(r#"{"first_level":"{\"second_level\":\"finish\"}"}"#),
            },
        ]
    }

    fn compile(
        &self,
        _state: (&mut state::LocalEnv, &mut state::ExternalEnv),
        _ctx: &mut FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");
        let max_depth = arguments.optional("max_depth");

        Ok(Box::new(ParseJsonFn { value, max_depth }))
    }

    fn call_by_vm(&self, _ctx: &mut Context, args: &mut VmArgumentList) -> Resolved {
        let value = args.required("value");
        let max_depth = args.optional("max_depth");
        parse_json(value, max_depth)
    }
}

#[derive(Debug, Clone)]
struct ParseJsonFn {
    value: Box<dyn Expression>,
    max_depth: Option<Box<dyn Expression>>,
}

impl Expression for ParseJsonFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        let max_depth = self
            .max_depth
            .as_ref()
            .map(|c| c.resolve(ctx))
            .transpose()?;
        parse_json(value, max_depth)
    }

    fn type_def(&self, _: (&state::LocalEnv, &state::ExternalEnv)) -> TypeDef {
        type_def()
    }
}

fn inner_kind() -> Kind {
    Kind::null()
        | Kind::bytes()
        | Kind::integer()
        | Kind::float()
        | Kind::boolean()
        | Kind::array(Collection::any())
        | Kind::object(Collection::any())
}

fn type_def() -> TypeDef {
    TypeDef::bytes()
        .fallible()
        .add_boolean()
        .add_integer()
        .add_float()
        .add_null()
        .add_array(Collection::from_unknown(inner_kind()))
        .add_object(Collection::from_unknown(inner_kind()))
}

#[cfg(test)]
mod tests {
    use super::*;

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
            tdef: TypeDef::bytes().fallible()
                .add_boolean()
                .add_integer()
                .add_float()
                .add_null()
                .add_array(Collection::from_unknown(inner_kind()))
                .add_object(Collection::from_unknown(inner_kind())),
        }

        max_depth {
            args: func_args![ value: r#"{"top_layer": {"layer_one": "finish", "layer_two": 2}}"#, max_depth: 1],
            want: Ok(value!({ top_layer: r#"{"layer_one": "finish", "layer_two": 2}"# })),
            tdef: type_def(),
        }

        max_depth_array {
            args: func_args![ value: r#"[{"top_layer": {"next_layer": ["finish"]}}]"#, max_depth: 2],
            want: Ok(value!([{ top_layer: r#"{"next_layer": ["finish"]}"# }])),
            tdef: type_def(),
        }

        max_depth_exhausted {
            args: func_args![ value: r#"{"top_layer": {"layer_one": "finish", "layer_two": 2}}"#, max_depth: 10],
            want: Ok(value!({ top_layer: {layer_one: "finish", layer_two: 2} })),
            tdef: type_def(),
        }

        invalid_json_with_max_depth {
            args: func_args![ value: r#"{"field": "value"#, max_depth: 3 ],
            want: Err("unable to read json: EOF while parsing a string at line 1 column 16"),
            tdef: TypeDef::bytes().fallible()
                .add_boolean()
                .add_integer()
                .add_float()
                .add_null()
                .add_array(Collection::from_unknown(inner_kind()))
                .add_object(Collection::from_unknown(inner_kind())),
        }

        invalid_input_max_depth {
            args: func_args![ value: r#"{"top_layer": "finish"}"#, max_depth: 129],
            want: Err("max_depth value should be greater than 0 and less than 128, got 129"),
            tdef: type_def(),
        }
    ];
}
