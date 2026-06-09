use serde_json::{Number, Value};
use snafu::{OptionExt, Snafu};
use std::collections::HashSet;

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

    #[snafu(display("Coercion failed at '{path}': {message}"))]
    Coerce { path: String, message: String },
}

use pastey::paste;

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
    handle_one_of(value, schema, definitions, path_components)?;
    handle_any_of(value, schema, definitions, path_components)?;
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

/// Apply `oneOf` semantics: exactly one schema must match,
/// or, if the schema is marked `"untagged"`, behave like `anyOf`.
fn handle_one_of(
    value: &mut Value,
    schema: &Value,
    definitions: Option<&Value>,
    path_components: &mut Vec<String>,
) -> Result<(), Error> {
    let Some(variants) = schema.get("oneOf").and_then(|v| v.as_array()) else {
        return Ok(());
    };

    // If this oneOf is marked untagged, treat it like anyOf
    let is_untagged = schema
        .get("_metadata")
        .and_then(|m| m.get("docs::enum_tagging"))
        .and_then(Value::as_str)
        == Some("untagged");

    if is_untagged {
        coerce_any_of(value, variants, definitions, path_components)
    } else {
        coerce_one_of(value, variants, definitions, path_components)
    }
}

/// Apply `anyOf` semantics: at least one schema must match.
fn handle_any_of(
    value: &mut Value,
    schema: &Value,
    definitions: Option<&Value>,
    path_components: &mut Vec<String>,
) -> Result<(), Error> {
    let Some(variants) = schema.get("anyOf").and_then(|v| v.as_array()) else {
        return Ok(());
    };
    coerce_any_of(value, variants, definitions, path_components)
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
        if let Ok(b) = s_trimmed.parse::<bool>()
            && enum_vals.iter().any(|opt| opt.as_bool() == Some(b))
        {
            *value = Value::Bool(b);
            return Ok(());
        }

        // String → Number
        if let Some(n) = parse_number(s_trimmed)
            && enum_vals.iter().any(|opt| opt.as_number() == Some(&n))
        {
            *value = Value::Number(n);
            return Ok(());
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
        if let Ok(b) = s_trimmed.parse::<bool>()
            && const_val.as_bool() == Some(b)
        {
            *value = Value::Bool(b);
            return Ok(());
        }

        // String → Number
        if let Some(n) = parse_number(s_trimmed)
            && const_val.as_number() == Some(&n)
        {
            *value = Value::Number(n);
            return Ok(());
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
            // Any type can be wrapped to an array. This is needed because  we have deserialization logic that accepts
            // e.g. a single string and converts it to an array, set or some other collection.
            if !value.is_array() {
                *value = Value::Array(vec![value.clone()]);
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

    // When unevaluatedProperties:false is set (used by Vector's component outer-wrapper schemas
    // like SourceOuter, SinkOuter, EnrichmentTableOuter), collect all properties declared
    // anywhere in the schema — including across allOf/$ref/oneOf — and flag any key not found.
    // This is the right level to do this check: the outer wrapper sees the full union of all
    // properties (component-specific fields + shared fields like `inputs`, `proxy`, `graph`).
    //
    // Guard: skip the check when the value's `type` discriminant is not recognized by any
    // compiled variant. In that case the component is simply not compiled in, and checking
    // would incorrectly flag its fields as unknown.
    let unevaluated_props_false = schema
        .get("unevaluatedProperties")
        .and_then(|v| v.as_bool())
        == Some(false);
    let known_for_unevaluated: Option<HashSet<String>> = if unevaluated_props_false {
        let type_val = obj.get("type").and_then(|t| t.as_str());
        let type_is_known = type_val
            .map(|tv| schema_contains_type_discriminant(schema, definitions, tv))
            .unwrap_or(false); // no `type` field → not a component config → skip check
        if type_is_known {
            let mut set = HashSet::new();
            // Filter `oneOf` variants by the value's discriminant so properties
            // valid in *other* variants (e.g. a Kafka-only field on an HTTP sink)
            // still trigger the unknown-field check.
            collect_known_properties(schema, definitions, type_val, &mut set);
            Some(set)
        } else {
            None
        }
    } else {
        None
    };

    for key in obj.clone().keys() {
        let key_str = key.as_str();
        let initial_len = path_components.len();
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
                path_components.truncate(initial_len);
                return UnexpectedPropertySnafu {
                    path: path_components.join("."),
                    key: key_str.to_string(),
                }
                .fail();
            }

            let result = coerce(
                obj.get_mut(key_str).unwrap(),
                field_schema,
                definitions,
                path_components,
            );
            path_components.truncate(initial_len);
            result?;
        } else {
            if let Some(ref known) = known_for_unevaluated
                && !known.contains(key_str)
            {
                // Vector's generated JSON Schema currently does not emit
                // `#[serde(alias = "...")]` aliases (TODO in vector-config),
                // so a key absent from the schema may still be a legitimate
                // serde alias. Warn here for visibility; serde performs the
                // authoritative unknown-field check downstream where it has
                // alias information.
                warn!(
                    message = "Unknown field in config, deferring to serde for alias resolution.",
                    path = %path_components.join("."),
                );
            }
            path_components.truncate(initial_len);
        }
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
                let initial_len = path.len();
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
                        path.truncate(initial_len);
                        return UnexpectedArrayElementSnafu {
                            path: path.join("."),
                            index: idx,
                        }
                        .fail();
                    }

                    let result = coerce(item_val, item_schema, definitions, path);
                    path.truncate(initial_len);
                    result?;
                } else {
                    path.truncate(initial_len);
                }
            }
        }

        item_schema => {
            for (idx, item_val) in arr.iter_mut().enumerate() {
                let initial_len = path.len();
                path.push(idx.to_string());
                let result = coerce(item_val, item_schema, definitions, path);
                path.truncate(initial_len);
                result?;
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
    let initial_len = path_components.len();
    let mut success: Option<(Value, &Value)> = None; // (coerced value, matched schema)

    for schema in schemas {
        path_components.truncate(initial_len);
        let mut candidate = value.clone();
        if coerce(&mut candidate, schema, definitions, path_components).is_ok() {
            path_components.truncate(initial_len);
            if success.is_some() {
                // Multiple variants match — keep the first and move on.
                break;
            }
            success = Some((candidate, schema));
        }
    }

    path_components.truncate(initial_len);

    if let Some((val, _matched_schema)) = success {
        *value = val;
        return Ok(());
    }

    // No variant succeeded. If the value carries a `type` discriminant that matches a known
    // variant, re-run coercion strictly against that variant so callers get a path-aware
    // coercion error (e.g. "expected integer at sources.my_source.count") rather than a
    // silent pass-through. Unknown-field detection is handled at the outer wrapper level
    // (see unevaluatedProperties handling in coerce_object).
    if let Some(type_val) = value
        .as_object()
        .and_then(|o| o.get("type"))
        .and_then(|t| t.as_str())
    {
        for schema in schemas {
            if schema_matches_type_discriminant(schema, definitions, type_val) {
                let mut candidate = value.clone();
                let result = coerce(&mut candidate, schema, definitions, path_components);
                path_components.truncate(initial_len);
                return result.map(|_| {
                    *value = candidate;
                });
            }
        }
    }

    Ok(())
}

/// Collect property names declared in `schema`, recursively through `$ref`, `allOf`,
/// `anyOf`, and `oneOf`. When `discriminant` is `Some`, `oneOf` traversal is filtered
/// to only the variant whose `properties.type.const` matches — so unknown-field
/// detection on a tagged-union component doesn't accept fields that are valid only
/// in *other* variants.
fn collect_known_properties<'a>(
    schema: &'a Value,
    definitions: Option<&'a Value>,
    discriminant: Option<&str>,
    out: &mut HashSet<String>,
) {
    let resolved = if let Some(ref_str) = schema.get("$ref").and_then(|r| r.as_str()) {
        let key = ref_str.strip_prefix("#/definitions/").unwrap_or("");
        match definitions.and_then(|d| d.get(key)) {
            Some(def) => def,
            None => return,
        }
    } else {
        schema
    };

    if let Some(props) = resolved.get("properties").and_then(|p| p.as_object()) {
        out.extend(props.keys().cloned());
    }

    for kw in ["allOf", "anyOf"] {
        if let Some(variants) = resolved.get(kw).and_then(|v| v.as_array()) {
            for sub in variants {
                collect_known_properties(sub, definitions, discriminant, out);
            }
        }
    }

    if let Some(variants) = resolved.get("oneOf").and_then(|v| v.as_array()) {
        match discriminant {
            Some(disc) => {
                // Only collect properties from the variant whose `type` const matches.
                // If no variant matches (e.g. untagged or non-component oneOf), fall back
                // to including all variants so callers don't get false positives.
                let matched: Vec<&Value> = variants
                    .iter()
                    .filter(|v| schema_matches_type_discriminant(v, definitions, disc))
                    .collect();
                if matched.is_empty() {
                    for sub in variants {
                        collect_known_properties(sub, definitions, discriminant, out);
                    }
                } else {
                    for sub in matched {
                        collect_known_properties(sub, definitions, discriminant, out);
                    }
                }
            }
            None => {
                for sub in variants {
                    collect_known_properties(sub, definitions, discriminant, out);
                }
            }
        }
    }
}

/// Returns true if `schema` (after resolving any `$ref` and walking `allOf`) has a
/// `properties.type.const` equal to `expected`.
fn schema_matches_type_discriminant(
    schema: &Value,
    definitions: Option<&Value>,
    expected: &str,
) -> bool {
    let resolved = if let Some(ref_str) = schema.get("$ref").and_then(|r| r.as_str()) {
        let key = ref_str.strip_prefix("#/definitions/").unwrap_or("");
        match definitions.and_then(|d| d.get(key)) {
            Some(def) => def,
            None => return false,
        }
    } else {
        schema
    };

    if resolved
        .get("properties")
        .and_then(|p| p.get("type"))
        .and_then(|t| t.get("const"))
        .and_then(|c| c.as_str())
        == Some(expected)
    {
        return true;
    }

    // Walk allOf in case the discriminant is embedded there.
    if let Some(all_of) = resolved.get("allOf").and_then(|v| v.as_array())
        && all_of
            .iter()
            .any(|sub| schema_matches_type_discriminant(sub, definitions, expected))
    {
        return true;
    }

    false
}

/// Returns true if `schema`, after fully resolving `$ref`, `allOf`, and `oneOf`, contains
/// any variant that claims `expected` as its `type` discriminant. Used to skip the
/// unevaluatedProperties unknown-field check for components whose type is not compiled in.
fn schema_contains_type_discriminant(
    schema: &Value,
    definitions: Option<&Value>,
    expected: &str,
) -> bool {
    let resolved = if let Some(ref_str) = schema.get("$ref").and_then(|r| r.as_str()) {
        let key = ref_str.strip_prefix("#/definitions/").unwrap_or("");
        match definitions.and_then(|d| d.get(key)) {
            Some(def) => def,
            None => return false,
        }
    } else {
        schema
    };

    if schema_matches_type_discriminant(resolved, definitions, expected) {
        return true;
    }

    if let Some(all_of) = resolved.get("allOf").and_then(|v| v.as_array())
        && all_of
            .iter()
            .any(|sub| schema_contains_type_discriminant(sub, definitions, expected))
    {
        return true;
    }

    if let Some(one_of) = resolved.get("oneOf").and_then(|v| v.as_array())
        && one_of
            .iter()
            .any(|variant| schema_matches_type_discriminant(variant, definitions, expected))
    {
        return true;
    }

    false
}

/// Apply `anyOf` semantics: at least one schema must match.
fn coerce_any_of(
    value: &mut Value,
    schemas: &[Value],
    definitions: Option<&Value>,
    path_components: &mut Vec<String>,
) -> Result<(), Error> {
    let initial_len = path_components.len();

    for schema in schemas {
        path_components.truncate(initial_len);
        let mut candidate = value.clone();
        if coerce(&mut candidate, schema, definitions, path_components).is_ok() {
            path_components.truncate(initial_len);
            *value = candidate;
            return Ok(());
        }
    }

    path_components.truncate(initial_len);
    Ok(())
}

/// Parse a string into a serde_json Number (integer only). Returns None if not a valid integer.
fn parse_integer(input: &str) -> Option<Number> {
    if let Ok(i) = input.trim().parse::<i64>() {
        Some(Number::from(i))
    } else if let Ok(u) = input.trim().parse::<u64>() {
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

        if let Some(f) = n.as_f64()
            && f.fract() == 0.0
            && f >= (i64::MIN as f64)
            && f <= (i64::MAX as f64)
        {
            *value = Value::Number(Number::from(f as i64));
            return Ok(());
        }
    } else if let Value::String(s) = value
        && let Some(n) = parse_integer(s)
    {
        *value = Value::Number(n);
        return Ok(());
    }

    fail_expected!(Integer, value, path_components)
}

fn coerce_number(value: &mut Value, path_components: &mut [String]) -> Result<(), Error> {
    if let Value::Number(_) = value {
        return Ok(());
    }

    if let Value::String(s) = value
        && let Some(n) = parse_number(s)
    {
        *value = Value::Number(n);
        return Ok(());
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
    use crate::config::ConfigBuilder;
    use crate::config::loading::schema_coercion::coerce;
    use serde_json::json;
    use vector_config::schema::generate_root_schema;

    #[test]
    fn test_coercion_with_array_support() {
        let mut input = json!({
            "proxy": {
                "enabled": true,
                "http": "http://example.com",
                "https": "https://example.com",
                "no_proxy": "no-proxy.com"
            },
            "enrichment_tables": {
                "memory_table": {
                    "type": "memory",
                    "ttl": 60,
                    "flush_interval": 5,
                    "inputs": ["s0"],
                },
            },
            "secret": {
                "backend_1": {
                    "type": "file",
                    "path": "some.json",
                },
            },
            "sources": {
                "source0": {
                    "type": "demo_logs",
                    "count": "100",
                    "format": "shuffle",
                    "lines": ["777", true, false, 0.1, 123, "some string"],
                    "interval": "1",
                },
            },
            "transforms": {
                "t0": {
                    "inputs": ["s0"],
                    "type": "remap",
                    "source": ".host = \"${HOSTNAME}\""
                },
            },
            "sinks": {
                "sink0": {
                    "inputs": ["t0"],
                    "type": "console",
                    "encoding": {
                        "codec": "json",
                    },
                },
            },
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
              "proxy": {
                "enabled": true,
                "http": "http://example.com",
                "https": "https://example.com",
                "no_proxy": ["no-proxy.com"]
              },
              "enrichment_tables": {
                "memory_table": {
                  "type": "memory",
                  "ttl": 60,
                  "flush_interval": 5,
                  "inputs": [
                    "s0"
                  ]
                }
              },
              "secret": {
                "backend_1": {
                  "type": "file",
                  "path": "some.json"
                }
              },
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
              },
              "transforms": {
                "t0": {
                  "inputs": [
                    "s0"
                  ],
                  "type": "remap",
                  "source": ".host = \"${HOSTNAME}\""
                }
              },
              "sinks": {
                "sink0": {
                  "inputs": [
                    "t0"
                  ],
                  "type": "console",
                  "encoding": {
                    "codec": "json"
                  }
                }
              }
            })
        );
    }

    #[test]
    fn test_unknown_field_in_known_component_passes_through() {
        // Unknown fields are intentionally non-fatal in the coercion pass while
        // `vector-config` does not emit `#[serde(alias = ...)]` aliases. The
        // pass logs a warning and defers to serde, which has alias info.
        let mut input = json!({
            "sources": {
                "source0": {
                    "type": "demo_logs",
                    "count": 100,
                    "totally_unknown_field": "oops",
                }
            }
        });

        let schema =
            serde_json::to_value(generate_root_schema::<ConfigBuilder>().unwrap()).unwrap();
        let result = coerce(
            &mut input,
            &schema,
            schema.get("definitions"),
            &mut Vec::new(),
        );

        assert!(
            result.is_ok(),
            "unknown field should pass coercion (serde validates downstream), got: {result:?}"
        );
    }

    #[test]
    fn test_unknown_component_type_passes_through() {
        let mut input = json!({
            "sinks": {
                "s3_sink": {
                    "type": "aws_s3_totally_nonexistent",
                    "bucket": "my-bucket",
                }
            }
        });

        let schema =
            serde_json::to_value(generate_root_schema::<ConfigBuilder>().unwrap()).unwrap();
        let result = coerce(
            &mut input,
            &schema,
            schema.get("definitions"),
            &mut Vec::new(),
        );

        assert!(
            result.is_ok(),
            "unknown component type should pass through coercion, got: {result:?}"
        );
    }
}
