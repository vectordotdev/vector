use std::collections::HashMap;

use ::value::Value;
use serde_json::{
    value::{RawValue, Value as JsonValue},
    Error, Map,
};
use vrl::prelude::*;

fn parse_json(value: Value) -> Resolved {
    let bytes = value.try_bytes()?;
    let value = serde_json::from_slice::<'_, Value>(&bytes)
        .map_err(|e| format!("unable to parse json: {e}"))?;
    Ok(value)
}

// parse_json_with_depth method recursively traverses the value and returns raw JSON-formatted bytes
// after reaching provided depth.
fn parse_json_with_depth(value: Value, max_depth: Value) -> Resolved {
    let bytes = value.try_bytes()?;
    let parsed_depth = validate_depth(max_depth)?;

    let raw_value = serde_json::from_slice::<'_, &RawValue>(&bytes)
        .map_err(|e| format!("unable to read json: {e}"))?;

    let res = parse_layer(raw_value, parsed_depth)
        .map_err(|e| format!("unable to parse json with max depth: {e}"))?;

    Ok(Value::from(res))
}

fn parse_layer(value: &RawValue, remaining_depth: u8) -> std::result::Result<JsonValue, Error> {
    let raw_value = value.get();

    // RawValue is a JSON object.
    if raw_value.starts_with('{') {
        if remaining_depth == 0 {
            // If max_depth is reached, return the raw representation of the JSON object,
            // e.g., "{\"key\":\"value\"}"
            serde_json::value::to_value(raw_value)
        } else {
            // Parse each value of the object as a raw JSON value recursively with the same method.
            let map: HashMap<String, &RawValue> = serde_json::from_str(raw_value)?;

            let mut res_map: Map<String, JsonValue> = Map::with_capacity(map.len());
            for (k, v) in map {
                res_map.insert(k, parse_layer(v, remaining_depth - 1)?);
            }
            Ok(serde_json::Value::from(res_map))
        }
    // RawValue is a JSON array.
    } else if raw_value.starts_with('[') {
        if remaining_depth == 0 {
            // If max_depth is reached, return the raw representation of the JSON array,
            // e.g., "[\"one\",\"two\",\"three\"]"
            serde_json::value::to_value(raw_value)
        } else {
            // Parse all values of the array as a raw JSON value recursively with the same method.
            let arr: Vec<&RawValue> = serde_json::from_str(raw_value)?;

            let mut res_arr: Vec<JsonValue> = Vec::with_capacity(arr.len());
            for v in arr {
                res_arr.push(parse_layer(v, remaining_depth - 1)?)
            }
            Ok(serde_json::Value::from(res_arr))
        }
    // RawValue is not an object or array, do not need to traverse the doc further.
    // Parse and return the value.
    } else {
        serde_json::from_str(raw_value)
    }
}

fn validate_depth(value: Value) -> std::result::Result<u8, ExpressionError> {
    let res = value.try_integer()?;

    // The lower cap is 1 because it is pointless to use anything lower,
    // because 'data = parse_json!(.message, max_depth: 0)' equals to 'data = .message'.
    //
    // The upper cap is 128 because serde_json has the same recursion limit by default.
    // https://github.com/serde-rs/json/blob/4d57ebeea8d791b8a51c229552d2d480415d00e6/json/src/de.rs#L111
    if (1..=128).contains(&res) {
        Ok(res as u8)
    } else {
        Err(ExpressionError::from(format!(
            "max_depth value should be greater than 0 and less than 128, got {}",
            res
        )))
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
        _state: &state::TypeState,
        _ctx: &mut FunctionCompileContext,
        arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");
        let max_depth = arguments.optional("max_depth");

        match max_depth {
            Some(max_depth) => Ok(ParseJsonMaxDepthFn { value, max_depth }.as_expr()),
            None => Ok(ParseJsonFn { value }.as_expr()),
        }
    }
}

#[derive(Debug, Clone)]
struct ParseJsonFn {
    value: Box<dyn Expression>,
}

impl FunctionExpression for ParseJsonFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        parse_json(value)
    }

    fn type_def(&self, _: &state::TypeState) -> TypeDef {
        type_def()
    }
}

#[derive(Debug, Clone)]
struct ParseJsonMaxDepthFn {
    value: Box<dyn Expression>,
    max_depth: Box<dyn Expression>,
}

impl FunctionExpression for ParseJsonMaxDepthFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        let max_depth = self.max_depth.resolve(ctx)?;
        parse_json_with_depth(value, max_depth)
    }

    fn type_def(&self, _: &state::TypeState) -> TypeDef {
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
        .or_boolean()
        .or_integer()
        .or_float()
        .add_null()
        .or_array(Collection::from_unknown(inner_kind()))
        .or_object(Collection::from_unknown(inner_kind()))
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
                .or_boolean()
                .or_integer()
                .or_float()
                .or_null()
                .or_array(Collection::from_unknown(inner_kind()))
                .or_object(Collection::from_unknown(inner_kind())),
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

        max_depth_exceeds_layers {
            args: func_args![ value: r#"{"top_layer": {"layer_one": "finish", "layer_two": 2}}"#, max_depth: 10],
            want: Ok(value!({ top_layer: {layer_one: "finish", layer_two: 2} })),
            tdef: type_def(),
        }

        invalid_json_with_max_depth {
            args: func_args![ value: r#"{"field": "value"#, max_depth: 3 ],
            want: Err("unable to read json: EOF while parsing a string at line 1 column 16"),
            tdef: TypeDef::bytes().fallible()
                .or_boolean()
                .or_integer()
                .or_float()
                .or_null()
                .or_array(Collection::from_unknown(inner_kind()))
                .or_object(Collection::from_unknown(inner_kind())),
        }

        invalid_input_max_depth {
            args: func_args![ value: r#"{"top_layer": "finish"}"#, max_depth: 129],
            want: Err("max_depth value should be greater than 0 and less than 128, got 129"),
            tdef: type_def(),
        }
    ];
}
