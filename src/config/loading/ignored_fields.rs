//! Diagnose unused configuration keys without changing which configurations Serde accepts.
//!
//! Serde buffers flattened structs and tagged enums before deserializing them, so wrappers such
//! as `serde_ignored` cannot observe all discarded fields. The generated schema describes the
//! fields of each input variant, including aliases and arbitrary maps. Evaluate a permissive
//! copy of that schema to collect the fields belonging to matching variants.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::LazyLock,
};

use jsonschema::Validator;
use serde_json::{Map, Value};
use vector_config::{
    constants::{METADATA, SERDE_ALIASES, SERDE_VARIANT_ALIASES},
    schema::generate_root_schema,
};

use super::ComponentHint;
use crate::config::ConfigBuilder;

const CLOSED_OBJECT: &str = "x-vector-closed-object";
const OPEN_OBJECT: &str = "x-vector-open-object";
const IGNORED_FIELD: &str = "x-vector-ignored-field";
const SERDE_IGNORED: &str = "serde::ignored";

static VALIDATOR: LazyLock<Option<Validator>> = LazyLock::new(|| {
    let result = generate_root_schema::<ConfigBuilder>()
        .map_err(|error| format!("{error:?}"))
        .and_then(|schema| serde_json::to_value(schema).map_err(|error| error.to_string()))
        .and_then(|mut schema| {
            prepare_schema(&mut schema);
            jsonschema::validator_for(&schema).map_err(|error| error.to_string())
        });
    match result {
        Ok(validator) => Some(validator),
        Err(error) => {
            warn!(%error, "Could not check configuration for ignored fields.");
            None
        }
    }
});

pub(super) fn find(map: &Map<String, Value>, hint: Option<ComponentHint>) -> Vec<String> {
    let Some(validator) = VALIDATOR.as_ref() else {
        return Vec::new();
    };
    let value = match hint {
        Some(ComponentHint::Test) => serde_json::json!({"tests": map.values().collect::<Vec<_>>()}),
        Some(hint) => serde_json::json!({hint.as_component_field(): map}),
        None => Value::Object(map.clone()),
    };
    ignored_paths(validator, &value)
}

pub(super) fn warnings(paths: &[String]) -> Vec<String> {
    paths
        .iter()
        .map(|path| {
            format!(
                "DEPRECATED: Configuration field `{path}` is ignored. Remove it; a future release will reject ignored fields. See https://vector.dev/deprecations/#ignored-configuration-fields."
            )
        })
        .collect()
}

#[derive(Default)]
struct ObjectFields {
    closed: bool,
    open: bool,
    evaluated: BTreeSet<String>,
}

fn ignored_paths(validator: &Validator, value: &Value) -> Vec<String> {
    let evaluation = validator.evaluate(value);
    if !evaluation.flag().valid {
        // Serde remains authoritative. A schema mismatch is not evidence that a field is unused.
        debug!(
            "Skipped ignored-field diagnostics because the configuration did not match its input schema."
        );
        return Vec::new();
    }

    let mut objects: BTreeMap<String, ObjectFields> = BTreeMap::new();
    let mut ignored = BTreeSet::new();
    for annotation in evaluation.iter_annotations() {
        let path = annotation.instance_location.as_str();
        let annotations = annotation.annotations.value();
        if annotations.get(IGNORED_FIELD) == Some(&Value::Bool(true)) {
            ignored.insert(path.to_owned());
        }
        let object = objects.entry(path.to_owned()).or_default();
        object.closed |= annotations.get(CLOSED_OBJECT) == Some(&Value::Bool(true));
        object.open |= annotations.get(OPEN_OBJECT) == Some(&Value::Bool(true));
        if matches!(
            annotation.schema_location.rsplit('/').next(),
            Some("properties" | "patternProperties" | "additionalProperties")
        ) && let Some(names) = annotations.as_array()
        {
            object
                .evaluated
                .extend(names.iter().filter_map(Value::as_str).map(str::to_owned));
        }
    }

    for (path, fields) in objects {
        if fields.closed
            && !fields.open
            && let Some(object) = value.pointer(&path).and_then(Value::as_object)
        {
            for name in object
                .keys()
                .filter(|name| !fields.evaluated.contains(*name))
            {
                let escaped = name.replace('~', "~0").replace('/', "~1");
                ignored.insert(format!("{path}/{escaped}"));
            }
        }
    }
    ignored.into_iter().collect()
}

fn prepare_schema(schema: &mut Value) {
    let Some(object) = schema.as_object_mut() else {
        return;
    };
    add_aliases(object);
    let ignored = object
        .get(METADATA)
        .and_then(|metadata| metadata.get(SERDE_IGNORED))
        == Some(&Value::Bool(true));
    // Descriptions/defaults are irrelevant to field recognition and can be large annotations.
    for key in [
        METADATA,
        "title",
        "description",
        "default",
        "examples",
        "deprecated",
        "format",
        "minimum",
        "maximum",
        "exclusiveMinimum",
        "exclusiveMaximum",
        "multipleOf",
        "minLength",
        "maxLength",
        "pattern",
        "minItems",
        "maxItems",
        "uniqueItems",
        "minProperties",
        "maxProperties",
    ] {
        object.remove(key);
    }
    if ignored {
        object.insert(IGNORED_FIELD.into(), Value::Bool(true));
    }
    if object.remove("unevaluatedProperties") == Some(Value::Bool(false)) {
        object.insert(CLOSED_OBJECT.into(), Value::Bool(true));
    }
    match object.get("additionalProperties") {
        Some(Value::Bool(false)) => {
            object.remove("additionalProperties");
            object.insert(CLOSED_OBJECT.into(), Value::Bool(true));
        }
        Some(Value::Bool(true)) => {
            object.insert(OPEN_OBJECT.into(), Value::Bool(true));
        }
        _ => {}
    }
    // Input forms can overlap (for example an enum's named string and general string forms).
    // Recognize fields from every matching form rather than diagnosing an ambiguous value.
    if let Some(mut variants) = object.remove("oneOf") {
        // Feature-restricted builds can have no registered components of a given kind.
        if variants.as_array().is_some_and(Vec::is_empty) {
            variants = serde_json::json!([false]);
        }
        if object.contains_key("anyOf") {
            object
                .entry("allOf")
                .or_insert_with(|| serde_json::json!([]))
                .as_array_mut()
                .expect("allOf is an array")
                .push(serde_json::json!({"anyOf": variants}));
        } else {
            object.insert("anyOf".into(), variants);
        }
    }
    if let Some(Value::String(reference)) = object.get_mut("$ref") {
        // Rust generic type names in definition references contain characters forbidden in URIs.
        let mut url = url::Url::parse("https://vector.invalid/").expect("valid static schema URL");
        if let Some(fragment) = reference.strip_prefix('#') {
            url.set_fragment(Some(fragment));
            *reference = format!("#{}", url.fragment().expect("fragment was set"));
        }
    }
    prepare_subschemas(object);
}

fn prepare_subschemas(object: &mut Map<String, Value>) {
    for key in [
        "properties",
        "patternProperties",
        "definitions",
        "$defs",
        "dependentSchemas",
    ] {
        if let Some(children) = object.get_mut(key).and_then(Value::as_object_mut) {
            for child in children.values_mut() {
                prepare_schema(child);
            }
        }
    }
    for key in ["allOf", "anyOf", "prefixItems"] {
        if let Some(children) = object.get_mut(key).and_then(Value::as_array_mut) {
            for child in children {
                prepare_schema(child);
            }
        }
    }
    for key in [
        "additionalProperties",
        "items",
        "additionalItems",
        "contains",
        "propertyNames",
        "not",
        "if",
        "then",
        "else",
    ] {
        if let Some(child) = object.get_mut(key) {
            if let Some(children) = child.as_array_mut() {
                for child in children {
                    prepare_schema(child);
                }
            } else {
                prepare_schema(child);
            }
        }
    }
}

fn add_aliases(schema: &mut Map<String, Value>) {
    if let Some(aliases) = schema
        .get(METADATA)
        .and_then(|metadata| metadata.get(SERDE_VARIANT_ALIASES))
        .and_then(Value::as_array)
        .cloned()
    {
        if let Some(value) = schema.remove("const") {
            schema.insert(
                "enum".into(),
                Value::Array(std::iter::once(value).chain(aliases).collect()),
            );
        } else if let Some(values) = schema.get_mut("enum").and_then(Value::as_array_mut) {
            values.extend(aliases);
        }
    }
    let Some(properties) = schema.get_mut("properties").and_then(Value::as_object_mut) else {
        return;
    };
    let mut groups = Vec::new();
    let mut added = Vec::new();
    for (name, property) in properties.iter() {
        if let Some(aliases) = property
            .get(METADATA)
            .and_then(|metadata| metadata.get(SERDE_ALIASES))
            .and_then(Value::as_array)
        {
            let mut names = vec![name.clone()];
            for alias in aliases.iter().filter_map(Value::as_str) {
                names.push(alias.to_owned());
                added.push((alias.to_owned(), property.clone()));
            }
            groups.push(names);
        }
    }
    properties.extend(added);
    for names in groups {
        if let Some(required) = schema.get_mut("required").and_then(Value::as_array_mut)
            && let Some(index) = required
                .iter()
                .position(|name| name.as_str() == Some(names[0].as_str()))
        {
            required.remove(index);
            let alternatives: Vec<_> = names
                .iter()
                .map(|name| serde_json::json!({"required": [name]}))
                .collect();
            schema
                .entry("allOf")
                .or_insert_with(|| serde_json::json!([]))
                .as_array_mut()
                .expect("allOf is an array")
                .push(serde_json::json!({"anyOf": alternatives}));
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::{ignored_paths, prepare_schema};

    fn ignored(mut schema: Value, value: &Value) -> Vec<String> {
        prepare_schema(&mut schema);
        let validator = jsonschema::validator_for(&schema).unwrap();
        ignored_paths(&validator, value)
    }

    #[test]
    fn empty_component_unions_do_not_disable_diagnostics() {
        let mut schema = json!({
            "type": "object", "additionalProperties": false,
            "properties": {
                "transforms": {"type": "object", "additionalProperties": {"oneOf": []}}
            }
        });
        prepare_schema(&mut schema);
        let validator = jsonschema::validator_for(&schema).unwrap();
        assert_eq!(
            ignored_paths(&validator, &json!({"transforms": {}, "typo": true})),
            ["/typo"]
        );
        assert!(!validator.is_valid(&json!({"transforms": {"unavailable": {}}})));
    }

    #[test]
    fn recognizes_flattened_fields_and_arbitrary_maps() {
        let schema = json!({
            "type": "object", "unevaluatedProperties": false,
            "allOf": [
                {"properties": {"timeout": {"type": "integer"}}},
                {"properties": {"headers": {"type": "object", "additionalProperties": {"type": "string"}}}}
            ]
        });
        assert_eq!(
            ignored(
                schema,
                &json!({"timeout": 30, "headers": {"X-Custom": "secret"}, "timeuot": 1})
            ),
            ["/timeuot"]
        );
    }

    #[test]
    fn recognizes_required_field_and_variant_aliases() {
        let schema = json!({
            "type": "object", "unevaluatedProperties": false, "required": ["endpoint"],
            "properties": {
                "endpoint": {"type": "string", "_metadata": {"serde::aliases": ["host"]}},
                "mode": {"const": "bulk", "_metadata": {"serde::variant_aliases": ["normal"]}}
            }
        });
        assert_eq!(
            ignored(
                schema,
                &json!({"host": "localhost", "mode": "normal", "typo": true})
            ),
            ["/typo"]
        );
    }

    #[test]
    fn warns_only_about_fields_unused_by_the_selected_variant() {
        let schema = json!({
            "type": "object", "unevaluatedProperties": false,
            "oneOf": [
                {"required": ["mode"], "properties": {"mode": {"const": "tcp"}, "address": {"type": "string"}}},
                {"required": ["mode"], "properties": {"mode": {"const": "unix"}, "path": {"type": "string"}}}
            ]
        });
        assert_eq!(
            ignored(
                schema,
                &json!({"mode": "tcp", "address": "127.0.0.1:9000", "path": "ignored"})
            ),
            ["/path"]
        );
    }

    #[test]
    fn explicitly_ignored_fields_are_reported_without_values() {
        let schema = json!({
            "type": "object", "unevaluatedProperties": false,
            "properties": {"password": {"type": "string", "_metadata": {"serde::ignored": true}}}
        });
        assert_eq!(
            ignored(schema, &json!({"password": "DO_NOT_LOG_THIS"})),
            ["/password"]
        );
    }

    #[test]
    fn nested_arrays_and_escaped_names_produce_unambiguous_paths() {
        let schema = json!({
            "type": "array", "items": {"$ref": "#/definitions/Config<String>"},
            "definitions": {"Config<String>": {"type": "object", "unevaluatedProperties": false}}
        });
        assert_eq!(ignored(schema, &json!([{"a~/b": "secret"}])), ["/0/a~0~1b"]);
    }

    #[test]
    fn does_not_warn_for_open_maps_or_unrecognized_input_shapes() {
        let schema =
            json!({"type": "object", "additionalProperties": true, "unevaluatedProperties": false});
        assert!(ignored(schema, &json!({"anything": {"nested": true}})).is_empty());
        let schema =
            json!({"type": "object", "required": ["required"], "unevaluatedProperties": false});
        assert!(ignored(schema, &json!({"anything": true})).is_empty());
    }

    #[test]
    fn arbitrary_values_and_overlapping_scalar_variants_remain_valid() {
        let schema = json!({
            "type": "object", "unevaluatedProperties": false,
            "properties": {
                "payload": {},
                "timezone": {"oneOf": [{"const": "local"}, {"type": "string"}]},
                "limit": {"type": "integer", "maximum": 9_007_199_254_740_991_u64}
            }
        });
        assert!(
            ignored(
                schema,
                &json!({"payload": {"any": {"key": true}}, "timezone": "local", "limit": u64::MAX})
            )
            .is_empty()
        );
    }
}
