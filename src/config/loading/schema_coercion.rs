use serde_json::{Number, Value};
use snafu::{OptionExt, Snafu};

const NULL_JSON_TYPE: &str = "null";
const BOOL_JSON_TYPE: &str = "boolean";
const NUMBER_JSON_TYPE: &str = "number";
const STRING_JSON_TYPE: &str = "string";
const ARRAY_JSON_TYPE: &str = "array";
const OBJECT_JSON_TYPE: &str = "object";

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("Expected boolean at '{path}', found '{actual}'"))]
    ExpectedBool { path: String, actual: &'static str },

    #[snafu(display("Expected integer at '{path}', found '{actual}'"))]
    ExpectedInteger { path: String, actual: &'static str },

    #[snafu(display("Expected number at '{path}', found '{actual}'"))]
    ExpectedNumber { path: String, actual: &'static str },

    #[snafu(display("Expected string at '{path}', found '{actual}'"))]
    ExpectedString { path: String, actual: &'static str },

    #[snafu(display("Expected null at '{path}', found '{actual}'"))]
    ExpectedNull { path: String, actual: &'static str },

    #[snafu(display("Expected array at '{path}', found '{actual}'"))]
    ExpectedArray { path: String, actual: &'static str },

    #[snafu(display("Expected object at '{path}', found '{actual}'"))]
    ExpectedObject { path: String, actual: &'static str },

    #[snafu(display("Unexpected property '{key}' at '{path}'"))]
    UnexpectedProperty { path: String, key: String },

    #[snafu(display("Unexpected extra array element at '{path}[{index}]'"))]
    UnexpectedArrayElement { path: String, index: usize },

    #[snafu(display("Schema reference '{reference}' not found at path '{path}'"))]
    SchemaReferenceNotFound { path: String, reference: String },

    #[snafu(display("Unsupported schema reference '{reference}' at path '{path}'"))]
    UnsupportedSchemaReference { path: String, reference: String },

    #[snafu(display("Unexpected property '{path}'"))]
    DisallowedProperty { path: String },

    #[snafu(display("Value at '{path}' is not one of the allowed enum options"))]
    InvalidEnumValue { path: String },

    #[snafu(display("Value at '{path}' does not match required constant '{expected}'"))]
    InvalidConst { path: String, expected: String },

    #[snafu(display("Value at '{path}' does not match any expected schema"))]
    OneOfNoMatch { path: String },

    #[snafu(display("Ambiguous value at '{path}': matches multiple schemas"))]
    OneOfMultipleMatches { path: String },

    #[snafu(display("Coercion failed at '{path}': {message}"))]
    Coerce { path: String, message: String },
}

use paste::paste;

macro_rules! fail_expected {
    ($variant:ident, $val:expr, $path_components:expr) => {
        paste! {
            [<Expected $variant Snafu>] {
                path: $path_components.join("."),
                actual: get_json_type($val),
            }
            .fail()
        }
    };
}

/// Recursively coerce `value` according to `schema`.
/// `definitions` is an optional reference to the root "definitions" map in the schema (for resolving $ref).
pub fn coerce(
    value: &mut Value,
    schema: &Value,
    definitions: Option<&Value>,
    path_components: &mut Vec<String>,
) -> Result<(), Error> {
    handle_bool(schema, path_components)?;
    handle_ref(value, schema, definitions, path_components)?;
    handle_all_of(value, schema, definitions, path_components)?;
    handle_one_of_any_of(value, schema, definitions, path_components)?;
    handle_enum(value, schema, path_components)?;
    handle_const(value, schema, path_components)?;

    if let Some(type_spec) = schema.get("type") {
        if let Some(t) = type_spec.as_str() {
            return coerce_type(value, t, schema, definitions, path_components);
        } else if let Some(types) = type_spec.as_array() {
            let allowed: Vec<&str> = types.iter().filter_map(|t| t.as_str()).collect();
            return coerce_multiple_types(value, &allowed, schema, definitions, path_components);
        }
    }

    // Only fall back to object/array coercion if no type is specified.
    match value {
        Value::Object(_) => coerce_object(value, schema, definitions, path_components),
        Value::Array(_) => coerce_array(value, schema, definitions, path_components),
        _ => Ok(()),
    }
}

fn handle_ref(
    value: &mut Value,
    schema: &Value,
    definitions: Option<&Value>,
    path_components: &mut Vec<String>,
) -> Result<(), Error> {
    let Some(ref_str) = schema.get("$ref").and_then(|r| r.as_str()) else {
        return Ok(());
    };

    let prefix = "#/definitions/";
    if let Some(key) = ref_str.strip_prefix(prefix) {
        let def_schema = definitions
            .and_then(|d| d.as_object())
            .and_then(|defs| defs.get(key))
            .context(SchemaReferenceNotFoundSnafu {
                path: path_components.join("."),
                reference: ref_str.to_string(),
            })?;

        coerce(value, def_schema, definitions, path_components)?;
        Ok(())
    } else {
        UnsupportedSchemaReferenceSnafu {
            path: path_components.join("."),
            reference: ref_str.to_string(),
        }
        .fail()
    }
}

fn handle_bool(schema: &Value, path_components: &mut [String]) -> Result<(), Error> {
    match schema.as_bool() {
        Some(true) | None => Ok(()),
        Some(false) => DisallowedPropertySnafu {
            path: path_components.join("."),
        }
        .fail(),
    }
}

fn handle_all_of(
    value: &mut Value,
    schema: &Value,
    definitions: Option<&Value>,
    path_components: &mut Vec<String>,
) -> Result<(), Error> {
    let Some(all_of) = schema.get("allOf").and_then(|v| v.as_array()) else {
        return Ok(());
    };

    for sub_schema in all_of {
        coerce(value, sub_schema, definitions, path_components)?;
    }

    Ok(())
}

fn handle_one_of_any_of(
    value: &mut Value,
    schema: &Value,
    definitions: Option<&Value>,
    path_components: &mut Vec<String>,
) -> Result<(), Error> {
    if let Some(one_of) = schema.get("oneOf").and_then(|v| v.as_array()) {
        return coerce_one_of(value, one_of, definitions, path_components);
    }

    if let Some(any_of) = schema.get("anyOf").and_then(|v| v.as_array()) {
        return coerce_any_of(value, any_of, definitions, path_components);
    }

    Ok(())
}

fn handle_enum(
    value: &mut Value,
    schema: &Value,
    path_components: &mut [String],
) -> Result<(), Error> {
    let Some(enum_vals) = schema.get("enum").and_then(|v| v.as_array()) else {
        return Ok(());
    };

    // Exact match
    if enum_vals.iter().any(|opt| value == opt) {
        return Ok(());
    }

    // Try coercions from string
    if let Value::String(s) = value {
        let s_trimmed = s.trim();

        // String → Bool
        if let Ok(b) = s_trimmed.parse::<bool>() {
            if enum_vals.iter().any(|opt| opt.as_bool() == Some(b)) {
                *value = Value::Bool(b);
                return Ok(());
            }
        }

        // String → Number
        if let Some(n) = parse_number(s_trimmed) {
            if enum_vals.iter().any(|opt| opt.as_number() == Some(&n)) {
                *value = Value::Number(n);
                return Ok(());
            }
        }

        // String → Null
        if s_trimmed.eq_ignore_ascii_case("null") && enum_vals.iter().any(|opt| opt.is_null()) {
            *value = Value::Null;
            return Ok(());
        }
    }

    // Number → String
    if let Value::Number(n) = value {
        let val_str = n.to_string();
        if enum_vals
            .iter()
            .any(|opt| opt.as_str() == Some(val_str.as_str()))
        {
            *value = Value::String(val_str);
            return Ok(());
        }
    }

    // Bool → String
    if let Value::Bool(b) = value {
        let val_str = b.to_string();
        if enum_vals.iter().any(|opt| {
            opt.as_str()
                .map(|s| s.eq_ignore_ascii_case(val_str.as_str()))
                == Some(true)
        }) {
            *value = Value::String(val_str);
            return Ok(());
        }
    }

    InvalidEnumValueSnafu {
        path: path_components.join("."),
    }
    .fail()
}

fn handle_const(
    value: &mut Value,
    schema: &Value,
    path_components: &mut [String],
) -> Result<(), Error> {
    let Some(const_val) = schema.get("const") else {
        return Ok(());
    };

    // Exact match
    if value == const_val {
        return Ok(());
    }

    // String input → try coercion
    if let Value::String(s) = value {
        let s_trimmed = s.trim();

        // String → Bool
        if let Ok(b) = s_trimmed.parse::<bool>() {
            if const_val.as_bool() == Some(b) {
                *value = Value::Bool(b);
                return Ok(());
            }
        }

        // String → Number
        if let Some(n) = parse_number(s_trimmed) {
            if const_val.as_number() == Some(&n) {
                *value = Value::Number(n);
                return Ok(());
            }
        }

        // String → Null
        if s_trimmed.eq_ignore_ascii_case("null") && const_val.is_null() {
            *value = Value::Null;
            return Ok(());
        }
    }

    // Number → String
    if let Value::Number(n) = value {
        let val_str = n.to_string();
        if const_val.as_str() == Some(val_str.as_str()) {
            *value = Value::String(val_str);
            return Ok(());
        }
    }

    // Bool → String
    if let Value::Bool(b) = value {
        let val_str = b.to_string();
        if const_val.as_str().map(|s| s.eq_ignore_ascii_case(&val_str)) == Some(true) {
            *value = Value::String(val_str);
            return Ok(());
        }
    }

    InvalidConstSnafu {
        path: path_components.join("."),
        expected: const_val.to_string(),
    }
    .fail()
}

/// Ensure `value` matches one of the allowed types in `allowed`.
/// If needed, convert the value to one of those types.
fn coerce_multiple_types(
    value: &mut Value,
    allowed_types: &[&str],
    schema: &Value,
    definitions: Option<&Value>,
    path_components: &mut Vec<String>,
) -> Result<(), Error> {
    for allowed_type in allowed_types {
        let mut new_value = value.clone();

        let result = try_coerce_to_allowed_type(
            &mut new_value,
            allowed_type,
            schema,
            definitions,
            path_components,
        );

        if result.is_ok() {
            *value = new_value;
            return Ok(());
        }
    }

    CoerceSnafu {
        path: path_components.join("."),
        message: format!(
            "Expected {} but found {}",
            allowed_types.join(" or "),
            get_json_type(value)
        ),
    }
    .fail()
}

/// Ensure `value` matches the expected single `expected_type`. Converts the value if possible.
fn coerce_type(
    value: &mut Value,
    expected_type: &str,
    schema: &Value,
    definitions: Option<&Value>,
    path_components: &mut Vec<String>,
) -> Result<(), Error> {
    try_coerce_to_allowed_type(value, expected_type, schema, definitions, path_components)
}

fn try_coerce_to_allowed_type(
    value: &mut Value,
    allowed_type: &str,
    schema: &Value,
    definitions: Option<&Value>,
    path_components: &mut Vec<String>,
) -> Result<(), Error> {
    match allowed_type {
        "null" => coerce_null(value, path_components),
        "boolean" => coerce_bool(value, path_components),
        "integer" => coerce_integer(value, path_components),
        "number" => coerce_number(value, path_components),
        "string" => coerce_string(value, path_components),
        "object" => {
            if !value.is_object() {
                return fail_expected!(Object, value, path_components);
            }
            coerce_object(value, schema, definitions, path_components)
        }
        "array" => {
            if !value.is_array() {
                return fail_expected!(Array, value, path_components);
            }
            coerce_array(value, schema, definitions, path_components)
        }
        _ => Ok(()), // silently skip unknown types
    }
}

/// Coerce all entries of an object value according to the schema's properties and additionalProperties.
fn coerce_object(
    value: &mut Value,
    schema: &Value,
    definitions: Option<&Value>,
    path_components: &mut Vec<String>,
) -> Result<(), Error> {
    let actual = get_json_type(&value.clone());
    let obj = value.as_object_mut().context(ExpectedObjectSnafu {
        path: path_components.join("."),
        actual,
    })?;

    let properties = schema.get("properties").and_then(|p| p.as_object());
    let additional_properties = schema.get("additionalProperties");

    for key in obj.clone().keys() {
        let key_str = key.as_str();
        path_components.push(key_str.to_string());

        let field_schema = properties.and_then(|props| props.get(key_str)).or_else(|| {
            additional_properties.and_then(|additional| {
                if let Some(b) = additional.as_bool() {
                    if b {
                        None // allowed, no specific schema
                    } else {
                        Some(&Value::Bool(false)) // trigger error below
                    }
                } else {
                    Some(additional) // schema for additional_properties
                }
            })
        });

        if let Some(field_schema) = field_schema {
            if field_schema == &Value::Bool(false) {
                return UnexpectedPropertySnafu {
                    path: path_components.join("."),
                    key: key_str.to_string(),
                }
                .fail();
            }

            coerce(
                obj.get_mut(key_str).unwrap(),
                field_schema,
                definitions,
                path_components,
            )?;
        }

        path_components.pop();
    }

    Ok(())
}

/// Coerce all elements of an array value according to the schema's items definition.
fn coerce_array(
    value: &mut Value,
    schema: &Value,
    definitions: Option<&Value>,
    path: &mut Vec<String>,
) -> Result<(), Error> {
    let actual = get_json_type(value);
    let arr = value.as_array_mut().context(ExpectedArraySnafu {
        path: path.join("."),
        actual,
    })?;

    let items = schema.get("items");
    let additional_items = schema.get("additionalItems");

    let Some(items_schema) = items else {
        // No "items" schema means all values are accepted as-is
        return Ok(());
    };

    match items_schema {
        Value::Array(tuple_schemas) => {
            for (idx, item_val) in arr.iter_mut().enumerate() {
                path.push(idx.to_string());

                let schema = tuple_schemas.get(idx).or({
                    match additional_items {
                        Some(Value::Bool(false)) => Some(&Value::Bool(false)), // disallowed
                        Some(s) => Some(s),                                    // additional schema
                        None => None,                                          // no schema → allow
                    }
                });

                if let Some(item_schema) = schema {
                    if item_schema == &Value::Bool(false) {
                        return UnexpectedArrayElementSnafu {
                            path: path.join("."),
                            index: idx,
                        }
                        .fail();
                    }

                    coerce(item_val, item_schema, definitions, path)?;
                }

                path.pop();
            }
        }

        item_schema => {
            for (idx, item_val) in arr.iter_mut().enumerate() {
                path.push(idx.to_string());
                coerce(item_val, item_schema, definitions, path)?;
                path.pop();
            }
        }
    }

    Ok(())
}

/// Apply `oneOf` semantics: exactly one schema must match.
fn coerce_one_of(
    value: &mut Value,
    schemas: &[Value],
    definitions: Option<&Value>,
    path_components: &mut Vec<String>,
) -> Result<(), Error> {
    let mut success_val = None;

    for schema in schemas {
        let mut candidate = value.clone();
        if coerce(&mut candidate, schema, definitions, path_components).is_ok() {
            if success_val.is_some() {
                return OneOfMultipleMatchesSnafu {
                    path: path_components.join("."),
                }
                .fail();
            }
            success_val = Some(candidate);
        }
    }

    match success_val {
        Some(val) => {
            *value = val;
            Ok(())
        }
        None => OneOfNoMatchSnafu {
            path: path_components.join("."),
        }
        .fail(),
    }
}

/// Apply `anyOf` semantics: at least one schema must match.
fn coerce_any_of(
    value: &mut Value,
    schemas: &[Value],
    definitions: Option<&Value>,
    path_components: &mut Vec<String>,
) -> Result<(), Error> {
    for schema in schemas {
        let mut candidate = value.clone();
        if coerce(&mut candidate, schema, definitions, path_components).is_ok() {
            *value = candidate;
            return Ok(());
        }
    }

    OneOfNoMatchSnafu {
        path: path_components.join("."),
    }
    .fail()
}

/// Parse a string into a serde_json Number (integer only). Returns None if not a valid integer.
fn parse_integer(input: &str) -> Option<Number> {
    let bytes = input.trim().as_bytes();
    if let Ok(i) = lexical_core::parse::<i64>(bytes) {
        Some(Number::from(i))
    } else if let Ok(u) = lexical_core::parse::<u64>(bytes) {
        Some(Number::from(u))
    } else {
        None
    }
}

/// Parse a string into a serde_json Number (integer or float). Returns None if not a valid number.
fn parse_number(input: &str) -> Option<Number> {
    if let Some(num) = parse_integer(input) {
        return Some(num);
    }
    if let Ok(f) = input.trim().parse::<f64>() {
        return Number::from_f64(f);
    }
    None
}

fn coerce_bool(value: &mut Value, path_components: &mut [String]) -> Result<(), Error> {
    match value {
        Value::Bool(_) => Ok(()),
        Value::String(s) => match s.trim().parse::<bool>() {
            Ok(b) => {
                *value = Value::Bool(b);
                Ok(())
            }
            _ => fail_expected!(Bool, value, path_components),
        },
        _ => fail_expected!(Bool, value, path_components),
    }
}

fn coerce_integer(value: &mut Value, path_components: &mut [String]) -> Result<(), Error> {
    if let Value::Number(n) = value {
        if n.is_i64() || n.is_u64() {
            return Ok(());
        }

        if let Some(f) = n.as_f64() {
            if f.fract() == 0.0 && f >= (i64::MIN as f64) && f <= (i64::MAX as f64) {
                *value = Value::Number(Number::from(f as i64));
                return Ok(());
            }
        }
    } else if let Value::String(s) = value {
        if let Some(n) = parse_integer(s) {
            *value = Value::Number(n);
            return Ok(());
        }
    }

    fail_expected!(Integer, value, path_components)
}

fn coerce_number(value: &mut Value, path_components: &mut [String]) -> Result<(), Error> {
    if let Value::Number(_) = value {
        return Ok(());
    }

    if let Value::String(s) = value {
        if let Some(n) = parse_number(s) {
            *value = Value::Number(n);
            return Ok(());
        }
    }

    fail_expected!(Number, value, path_components)
}

fn coerce_null(value: &mut Value, path_components: &mut [String]) -> Result<(), Error> {
    match value {
        Value::Null => Ok(()),
        Value::String(s) if s.trim().eq_ignore_ascii_case("null") => {
            *value = Value::Null;
            Ok(())
        }
        _ => fail_expected!(Null, value, path_components),
    }
}

fn coerce_string(value: &mut Value, path_components: &mut [String]) -> Result<(), Error> {
    match value {
        Value::String(_) => Ok(()),
        Value::Null => fail_expected!(String, value, path_components),
        _ => {
            *value = Value::String(value.to_string());
            Ok(())
        }
    }
}

/// Helper to get a human-readable type name for a JSON value.
const fn get_json_type(val: &Value) -> &'static str {
    match val {
        Value::Null => NULL_JSON_TYPE,
        Value::Bool(_) => BOOL_JSON_TYPE,
        Value::Number(_) => NUMBER_JSON_TYPE,
        Value::String(_) => STRING_JSON_TYPE,
        Value::Array(_) => ARRAY_JSON_TYPE,
        Value::Object(_) => OBJECT_JSON_TYPE,
    }
}

#[cfg(all(test, feature = "sources-demo_logs",))]
mod test {
    use crate::config::loading::schema_coercion::coerce;
    use crate::config::ConfigBuilder;
    use serde_json::json;
    use vector_config::schema::generate_root_schema;

    #[test]
    fn test_coercion_with_array_support() {
        let mut input = json!({
            "sources": {
                "source0": {
                    "type": "demo_logs",
                    "count": "100",
                    "format": "shuffle",
                    "lines": ["777", true, false, 0.1, 123, "some string"],
                    "interval": "1"
                }
            }
        });

        let demo_logs_schema =
            serde_json::to_value(generate_root_schema::<ConfigBuilder>().unwrap()).unwrap();
        coerce(
            &mut input,
            &demo_logs_schema,
            demo_logs_schema.get("definitions"),
            &mut Vec::new(),
        )
        .unwrap();
        assert_eq!(
            input,
            json!({
                "sources": {
                    "source0": {
                        "type": "demo_logs",
                        "count": 100,
                        "format": "shuffle",
                        "lines": [
                            "777",
                            "true",
                            "false",
                            "0.1",
                            "123",
                            "some string"
                        ],
                        "interval": 1
                    }
                }
            })
        );
    }
}
