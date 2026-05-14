#[path = "schema_core.rs"]
pub mod core;
#[path = "schema_enum.rs"]
pub mod r#enum;
#[path = "schema_resolve.rs"]
pub mod resolve;
#[path = "schema_utils.rs"]
pub mod utils;

use anyhow::{Result, anyhow};
use indexmap::IndexMap;
use serde_json::Value;
use std::env;

pub struct SchemaContext {
    pub root_schema: Value,
    pub cue_binary_path: String,
    pub resolved_schema_cache: IndexMap<String, Value>,
    pub expanded_schema_cache: IndexMap<String, Value>,
}

impl SchemaContext {
    pub fn new(root_schema: Value) -> Result<Self> {
        let cue_binary_path = find_command_on_path("cue")
            .ok_or_else(|| anyhow!("Failed to find 'cue' binary on the current path."))?;
        Ok(Self {
            root_schema,
            cue_binary_path,
            resolved_schema_cache: IndexMap::new(),
            expanded_schema_cache: IndexMap::new(),
        })
    }
}

pub fn find_command_on_path(command: &str) -> Option<String> {
    let exts = env::var("PATHEXT")
        .unwrap_or_else(|_| String::new())
        .split(';')
        .map(std::string::ToString::to_string)
        .collect::<Vec<String>>();

    let path_var = env::var("PATH").unwrap_or_default();
    let paths = std::env::split_paths(&path_var);

    for path in paths {
        for ext in &exts {
            let mut expected = path.join(command);
            expected.set_extension(ext.replace('.', ""));

            if expected.is_file() {
                return expected.to_str().map(std::string::ToString::to_string);
            }
        }
    }
    None
}

pub fn json_type_str(value: &Value) -> &'static str {
    match value {
        Value::String(_) => "string",
        Value::Number(n) if n.is_f64() => "number",
        Value::Number(_) => "integer",
        Value::Bool(_) => "boolean",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
        Value::Null => "null",
    }
}

pub fn docs_type_str(value: &Value) -> &'static str {
    let type_str = json_type_str(value);
    if type_str == "boolean" {
        "bool"
    } else {
        type_str
    }
}

pub fn get_schema_metadata<'a>(schema: &'a Value, key: &str) -> Option<&'a Value> {
    schema.get("_metadata").and_then(|m| m.get(key))
}

pub fn get_schema_ref(schema: &Value) -> Option<&str> {
    schema.get("$ref").and_then(|r| r.as_str())
}

pub fn nested_merge(base: &mut Value, override_val: &Value) {
    if override_val.is_null() {
        return;
    }

    if base.is_null() {
        *base = override_val.clone();
        return;
    }

    if base.is_object() && override_val.is_object() {
        let base_obj = base.as_object_mut().unwrap();
        let over_obj = override_val.as_object().unwrap();
        for (k, v) in over_obj {
            let entry = base_obj.entry(k.clone()).or_insert_with(|| Value::Null);
            nested_merge(entry, v);
        }
    } else if base.is_array() && override_val.is_array() {
        let base_arr = base.as_array_mut().unwrap();
        let over_arr = override_val.as_array().unwrap();
        for val in over_arr {
            if !base_arr.contains(val) {
                base_arr.push(val.clone());
            }
        }
    } else {
        *base = override_val.clone();
    }
}

pub fn schema_aware_nested_merge(base: &mut Value, override_val: &Value) {
    if override_val.is_null() {
        return;
    }

    if base.is_null() {
        *base = override_val.clone();
        return;
    }

    if base.is_object() && override_val.is_object() {
        let base_obj = base.as_object_mut().unwrap();
        let over_obj = override_val.as_object().unwrap();

        for (k, v) in over_obj {
            if k == "const"
                && is_const_variant(v)
                && is_existing_const_variants(base_obj.get("const"))
            {
                let base_vals = std::mem::take(base_obj.get_mut("const").unwrap());
                let mut result = Vec::new();

                push_const_variants(&mut result, base_vals);
                push_const_variants(&mut result, v.clone());

                base_obj.insert("const".to_string(), Value::Array(result));
            } else {
                let entry = base_obj.entry(k.clone()).or_insert_with(|| Value::Null);
                schema_aware_nested_merge(entry, v);
            }
        }
    } else if base.is_array() && override_val.is_array() {
        let base_arr = base.as_array_mut().unwrap();
        let over_arr = override_val.as_array().unwrap();
        for val in over_arr {
            if !base_arr.contains(val) {
                base_arr.push(val.clone());
            }
        }
    } else {
        *base = override_val.clone();
    }
}

/// True for a `{value, ...}` const variant, or an array of such variants.
fn is_const_variant(value: &Value) -> bool {
    match value {
        Value::Object(o) => o.contains_key("value"),
        Value::Array(arr) => arr.iter().all(is_const_variant),
        _ => false,
    }
}

fn is_existing_const_variants(existing: Option<&Value>) -> bool {
    existing.is_some_and(is_const_variant)
}

fn push_const_variants(result: &mut Vec<Value>, value: Value) {
    match value {
        Value::Array(arr) => {
            for item in arr {
                if !result.contains(&item) {
                    result.push(item);
                }
            }
        }
        obj @ Value::Object(_) => {
            if !result.contains(&obj) {
                result.push(obj);
            }
        }
        _ => {}
    }
}
