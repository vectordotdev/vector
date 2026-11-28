use serde_json::{Value, json};
use vector_config::{Configurable, configurable_component, schema::generate_root_schema};

/// Object representation shared by ordinary and shorthand fields.
#[configurable_component]
#[configurable(deprecated)]
#[derive(Clone)]
#[serde(default, deny_unknown_fields)]
struct Options {
    /// Whether the option is enabled.
    enabled: bool,
    /// Timeout in seconds.
    timeout: u64,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            enabled: true,
            timeout: 10,
        }
    }
}

fn custom_options() -> Options {
    Options {
        timeout: 20,
        ..Options::default()
    }
}

// These fixtures exercise schema code generation without resolving or invoking the helper
// functions. The real helper's Serde contract is covered in vector-core's tests.
/// Qualified helper paths and unrelated deserializers in one schema.
#[configurable_component(no_deser)]
struct Config {
    /// Shorthand through the current crate's re-export.
    #[serde(default, deserialize_with = "crate::serde::bool_or_struct")]
    crate_path: Options,
    /// Shorthand through vector-core.
    #[serde(
        default = "custom_options",
        deserialize_with = "vector_core::serde::bool_or_struct"
    )]
    core_path: Options,
    /// Shorthand through vector-lib's re-export.
    #[serde(default, deserialize_with = "::vector_lib::serde::bool_or_struct")]
    lib_path: Options,
    /// The same type without a custom deserializer.
    #[serde(default)]
    ordinary: Options,
    /// A different helper sharing the final function name.
    #[serde(default, deserialize_with = "other::serde::bool_or_struct")]
    unrelated: Options,
    /// A bare name cannot be resolved by the derive macro.
    #[serde(default, deserialize_with = "bool_or_struct")]
    bare: Options,
    /// A helper on a field that is only serialized.
    #[serde(
        default,
        skip_deserializing,
        deserialize_with = "crate::serde::bool_or_struct"
    )]
    output_only: Options,
}

/// Flattening exposes object fields, not the helper's standalone boolean form.
#[configurable_component(no_deser)]
struct Flattened {
    #[serde(default, flatten, deserialize_with = "crate::serde::bool_or_struct")]
    options: Options,
}

fn schema<T: Configurable + 'static>() -> Value {
    serde_json::to_value(generate_root_schema::<T>().unwrap()).unwrap()
}

#[test]
fn qualified_helpers_accept_boolean_and_object_forms() {
    let schema = schema::<Config>();
    let validator = jsonschema::validator_for(&schema).unwrap();
    for field in ["crate_path", "core_path", "lib_path"] {
        for value in [
            json!(false),
            json!(true),
            json!({}),
            json!({"enabled": false, "timeout": 7}),
        ] {
            let input = json!({field: value});
            assert!(validator.is_valid(&input), "{input}: {schema}");
        }
        for value in [
            json!(1),
            json!(1.5),
            json!("false"),
            Value::Null,
            json!([false]),
            json!({"timeout": "invalid"}),
        ] {
            let input = json!({field: value});
            assert!(!validator.is_valid(&input), "{input}: {schema}");
        }
    }
}

#[test]
fn unrelated_deserializers_and_plain_fields_remain_object_only() {
    let schema = schema::<Config>();
    let validator = jsonschema::validator_for(&schema).unwrap();
    for field in ["ordinary", "unrelated", "bare", "output_only"] {
        assert!(
            validator.is_valid(&json!({field: {"enabled": false}})),
            "{field}"
        );
        for value in [false, true] {
            assert!(!validator.is_valid(&json!({field: value})), "{field}");
        }
    }
}

#[test]
fn shared_schema_preserves_defaults_descriptions_and_deprecation() {
    let schema = schema::<Config>();
    let first = &schema["properties"]["crate_path"];
    let second = &schema["properties"]["core_path"];
    assert_eq!(first["default"], json!({"enabled": true, "timeout": 10}));
    assert_eq!(second["default"], json!({"enabled": true, "timeout": 20}));
    assert_eq!(
        first["description"],
        "Shorthand through the current crate's re-export."
    );
    assert_eq!(second["description"], "Shorthand through vector-core.");
    assert_eq!(first["$ref"], second["$ref"]);
    let shared = schema
        .pointer(first["$ref"].as_str().unwrap().strip_prefix('#').unwrap())
        .unwrap();
    assert_eq!(shared["deprecated"], true);
    assert_eq!(shared["_metadata"]["docs::enum_tagging"], "untagged");
    assert_eq!(shared["oneOf"][0]["deprecated"], true);
}

#[test]
fn flattened_helper_remains_object_only() {
    let schema = schema::<Flattened>();
    let validator = jsonschema::validator_for(&schema).unwrap();
    assert!(validator.is_valid(&json!({"enabled": false, "timeout": 7})));
    for input in [json!(false), json!(true), Value::Null, json!([false])] {
        assert!(!validator.is_valid(&input), "{input}: {schema}");
    }
}
