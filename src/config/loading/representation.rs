use std::collections::HashMap;

use serde::de::{DeserializeOwned, IntoDeserializer};
use serde_json::{Map, Number, Value};

use super::Format;

pub(super) type ConfigMap = Map<String, Value>;

const LARGE_INTEGER_ERROR: &str =
    "integer values larger than i64::MAX are not supported in Vector configuration";
const NON_FINITE_FLOAT_ERROR: &str =
    "non-finite float values are not supported in Vector configuration";

pub(super) fn deserialize_config<T>(content: &str, format: Format) -> Result<T, Vec<String>>
where
    T: DeserializeOwned,
{
    let value = parse_config_value(content, format)?;

    deserialize_config_value(value)
}

pub(super) fn deserialize_config_value<T>(value: Value) -> Result<T, Vec<String>>
where
    T: DeserializeOwned,
{
    serde_path_to_error::deserialize(value.into_deserializer())
        .map_err(|error| vec![error.to_string()])
}

pub(super) fn parse_config_value(content: &str, format: Format) -> Result<Value, Vec<String>> {
    let value = match format {
        Format::Toml => toml::from_str(content)
            .map_err(|error| vec![error.to_string()])
            .and_then(toml_to_json)?,
        Format::Yaml if is_blank_or_comment_only_yaml(content) => Value::Object(ConfigMap::new()),
        Format::Yaml => serde_yaml::from_str::<serde_yaml::Value>(content)
            .and_then(|mut value| {
                value.apply_merge()?;
                Ok(value)
            })
            .map_err(|error| vec![error.to_string()])
            .and_then(|value| {
                if value.is_null() && is_empty_yaml_document(content) {
                    Ok(Value::Object(ConfigMap::new()))
                } else {
                    yaml_to_json(value)
                }
            })?,
        Format::Json => {
            let value = serde_json::from_str(content).map_err(|error| vec![error.to_string()])?;
            validate_json(&value)?;
            value
        }
    };

    Ok(value)
}

fn is_blank_or_comment_only_yaml(content: &str) -> bool {
    content.lines().all(|line| {
        let line = line.trim();
        line.is_empty() || line.starts_with('#')
    })
}

fn is_empty_yaml_document(content: &str) -> bool {
    let content = content.strip_prefix('\u{feff}').unwrap_or(content);

    content.lines().all(|line| {
        let line = line.trim();
        line.is_empty()
            || line.starts_with('#')
            || line.starts_with('%')
            || is_yaml_document_marker(line, "---")
            || is_yaml_document_marker(line, "...")
    })
}

fn is_yaml_document_marker(line: &str, marker: &str) -> bool {
    line.strip_prefix(marker).is_some_and(|suffix| {
        let suffix = suffix.trim_start();
        suffix.is_empty() || suffix.starts_with('#')
    })
}

fn toml_to_json(value: toml::Value) -> Result<Value, Vec<String>> {
    match value {
        toml::Value::String(value) => Ok(Value::String(value)),
        toml::Value::Integer(value) => Ok(Value::Number(value.into())),
        toml::Value::Float(value) => finite_float_to_json(value),
        toml::Value::Boolean(value) => Ok(Value::Bool(value)),
        toml::Value::Datetime(value) => Ok(Value::String(value.to_string())),
        toml::Value::Array(values) => values
            .into_iter()
            .map(toml_to_json)
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        toml::Value::Table(table) => table
            .into_iter()
            .map(|(key, value)| toml_to_json(value).map(|value| (key, value)))
            .collect::<Result<ConfigMap, _>>()
            .map(Value::Object),
    }
}

fn yaml_to_json(value: serde_yaml::Value) -> Result<Value, Vec<String>> {
    match value {
        serde_yaml::Value::Null => Ok(Value::Null),
        serde_yaml::Value::Bool(value) => Ok(Value::Bool(value)),
        serde_yaml::Value::Number(value) => yaml_number_to_json(&value),
        serde_yaml::Value::String(value) => Ok(Value::String(value)),
        serde_yaml::Value::Sequence(values) => values
            .into_iter()
            .map(yaml_to_json)
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        serde_yaml::Value::Mapping(mapping) => mapping
            .into_iter()
            .map(|(key, value)| {
                let serde_yaml::Value::String(key) = key else {
                    return Err(vec![
                        "YAML mapping keys must be strings in Vector configuration".to_string(),
                    ]);
                };
                yaml_to_json(value).map(|value| (key, value))
            })
            .collect::<Result<ConfigMap, _>>()
            .map(Value::Object),
        serde_yaml::Value::Tagged(_) => Err(vec![
            "YAML tags are not supported in Vector configuration".to_string(),
        ]),
    }
}

fn yaml_number_to_json(value: &serde_yaml::Number) -> Result<Value, Vec<String>> {
    if let Some(value) = value.as_i64() {
        Ok(Value::Number(value.into()))
    } else if value.is_u64() {
        Err(vec![LARGE_INTEGER_ERROR.to_string()])
    } else if let Some(value) = value.as_f64() {
        finite_float_to_json(value)
    } else {
        unreachable!("serde_yaml::Number must be an integer or float")
    }
}

fn finite_float_to_json(value: f64) -> Result<Value, Vec<String>> {
    Number::from_f64(value)
        .map(Value::Number)
        .ok_or_else(|| vec![NON_FINITE_FLOAT_ERROR.to_string()])
}

fn validate_json(value: &Value) -> Result<(), Vec<String>> {
    match value {
        Value::Number(number)
            if number
                .as_u64()
                .is_some_and(|number| number > i64::MAX as u64) =>
        {
            Err(vec![LARGE_INTEGER_ERROR.to_string()])
        }
        Value::Array(values) => values.iter().try_for_each(validate_json),
        Value::Object(map) => map.values().try_for_each(validate_json),
        _ => Ok(()),
    }
}

pub(super) fn merge_into_map(map: &mut ConfigMap, other: ConfigMap) -> Result<(), Vec<String>> {
    merge_into_map_at_path(map, other, &mut Vec::new(), None).map_err(|error| vec![error])
}

pub(super) struct DeferredConflict {
    pub(super) path: Vec<String>,
    values: [Value; 2],
    error: String,
    requires_coercion: bool,
}

/// Retains overwritten string/scalar conflicts until the full configuration is assembled.
pub(super) fn merge_maps_deferred(
    maps: impl IntoIterator<Item = ConfigMap>,
) -> Result<(ConfigMap, Vec<DeferredConflict>), Vec<String>> {
    let mut merged = ConfigMap::new();
    let mut conflicts = Vec::new();
    for map in maps {
        merge_into_map_at_path(&mut merged, map, &mut Vec::new(), Some(&mut conflicts))
            .map_err(|error| vec![error])?;
    }
    Ok((merged, conflicts))
}

pub(super) fn resolve_merge_conflicts(
    merged: ConfigMap,
    conflicts: Vec<DeferredConflict>,
    mut coerce: impl FnMut(&mut Value) -> Result<(), Vec<String>>,
) -> Result<ConfigMap, Vec<String>> {
    let incompatible: HashMap<_, _> = conflicts
        .iter()
        .filter(|conflict| conflict.requires_coercion)
        .map(|conflict| (conflict.path.clone(), conflict.error.clone()))
        .collect();
    let conflicts: Vec<_> = conflicts
        .into_iter()
        .filter_map(|mut conflict| {
            let error = incompatible.get(&conflict.path)?;
            conflict.error.clone_from(error);
            Some(conflict)
        })
        .collect();
    if conflicts.is_empty() {
        return Ok(merged);
    }

    let uncoerced = Value::Object(merged);
    let mut merged = uncoerced.clone();
    coerce(&mut merged)?;
    for conflict in conflicts {
        let pointer: String = conflict
            .path
            .iter()
            .map(|key| format!("/{}", key.replace('~', "~0").replace('/', "~1")))
            .collect();
        // Use the assembled component (including its type tag and required fields),
        // not the incomplete file that supplied each value. Check both sides: even
        // the replacement may have been overwritten again by a later file.
        for value in conflict.values {
            let was_string = value.is_string();
            let mut candidate = uncoerced.clone();
            let Some(field) = candidate.pointer_mut(&pointer) else {
                return Err(vec![conflict.error]);
            };
            *field = value;
            coerce(&mut candidate)?;

            match (candidate.pointer(&pointer), merged.pointer(&pointer)) {
                (Some(previous), Some(current))
                    if !previous.is_array()
                        && !previous.is_object()
                        // A native number/bool was never a valid replacement for a
                        // string field. Deferral only enables string-to-scalar input.
                        && (was_string || !previous.is_string())
                        && value_type(previous) == value_type(current) => {}
                _ => return Err(vec![conflict.error]),
            }
        }
    }

    deserialize_config_value(merged)
}

fn merge_into_map_at_path(
    map: &mut ConfigMap,
    other: ConfigMap,
    path: &mut Vec<String>,
    mut conflicts: Option<&mut Vec<DeferredConflict>>,
) -> Result<(), String> {
    for (name, value) in other {
        if let Some(existing) = map.remove(&name) {
            path.push(name.clone());
            let merged = merge_values_at_path(existing, value, path, conflicts.as_deref_mut())?;
            path.pop();
            map.insert(name, merged);
        } else {
            map.insert(name, value);
        }
    }

    Ok(())
}

pub(super) fn merge_values(value: Value, other: Value) -> Result<Value, Vec<String>> {
    merge_values_at_path(value, other, &mut Vec::new(), None).map_err(|error| vec![error])
}

pub(super) fn merge_values_deferred(
    value: Value,
    other: Value,
    conflicts: &mut Vec<DeferredConflict>,
) -> Result<Value, Vec<String>> {
    merge_values_at_path(value, other, &mut Vec::new(), Some(conflicts))
        .map_err(|error| vec![error])
}

fn merge_values_at_path(
    value: Value,
    other: Value,
    path: &mut Vec<String>,
    conflicts: Option<&mut Vec<DeferredConflict>>,
) -> Result<Value, String> {
    match (value, other) {
        (Value::Null, Value::Null) => Ok(Value::Null),
        (Value::Bool(_), Value::Bool(other)) => Ok(Value::Bool(other)),
        (Value::String(value), Value::String(other)) => {
            // Preserve string history in case a later native scalar exposes a type
            // conflict. Ordinary string-only overrides never need validation.
            if value != other
                && let Some(conflicts) = conflicts
            {
                conflicts.push(DeferredConflict {
                    path: path.clone(),
                    values: [Value::String(value), Value::String(other.clone())],
                    error: String::new(),
                    requires_coercion: false,
                });
            }
            Ok(Value::String(other))
        }
        (Value::Number(value), Value::Number(other))
            if number_type(&value) == number_type(&other) =>
        {
            Ok(Value::Number(other))
        }
        (Value::Array(mut value), Value::Array(other)) => {
            value.extend(other);
            Ok(Value::Array(value))
        }
        (Value::Object(mut value), Value::Object(other)) => {
            merge_into_map_at_path(&mut value, other, path, conflicts)?;
            Ok(Value::Object(value))
        }
        (value, other) => {
            let display_path = if path.is_empty() {
                "$".to_owned()
            } else {
                format!("$.{}", path.join("."))
            };
            let error = format!(
                "Incompatible types at path \"{display_path}\", expected \"{}\" received \"{}\".",
                value_type(&value),
                value_type(&other)
            );
            if (value.is_string() || other.is_string())
                && !value.is_array()
                && !value.is_object()
                && !other.is_array()
                && !other.is_object()
                && let Some(conflicts) = conflicts
            {
                conflicts.push(DeferredConflict {
                    path: path.clone(),
                    values: [value, other.clone()],
                    error,
                    requires_coercion: true,
                });
                Ok(other)
            } else {
                Err(error)
            }
        }
    }
}

fn value_type(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(number) => number_type(number),
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "table",
    }
}

fn number_type(number: &Number) -> &'static str {
    if number.is_i64() || number.is_u64() {
        "integer"
    } else {
        "float"
    }
}

#[cfg(test)]
mod tests {
    use indoc::indoc;
    use serde::Deserialize;
    use serde_json::{Value, json};

    use super::{
        ConfigMap, deserialize_config, merge_maps_deferred, merge_values, parse_config_value,
        resolve_merge_conflicts,
    };
    use crate::config::{Format, loading::schema_coercion::ValueCoercer};

    #[derive(Debug, Deserialize, PartialEq)]
    struct StringValue {
        value: String,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct OptionalValue {
        optional: Option<String>,
        required: String,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct DefaultedOptionalValue {
        #[serde(default = "default_optional_value")]
        value: Option<String>,
    }

    #[derive(Debug, Deserialize)]
    struct NestedValue {
        #[serde(rename = "outer")]
        _outer: NestedInnerValue,
    }

    #[derive(Debug, Deserialize)]
    struct NestedInnerValue {
        #[serde(rename = "count")]
        _count: u64,
    }

    fn default_optional_value() -> Option<String> {
        Some("default".to_string())
    }

    #[test]
    fn supported_formats_deserialize_equivalent_values() {
        for (input, format) in [
            (r#"required = "present""#, Format::Toml),
            ("required: present", Format::Yaml),
            (r#"{"required": "present"}"#, Format::Json),
        ] {
            assert_eq!(
                deserialize_config::<OptionalValue>(input, format).unwrap(),
                OptionalValue {
                    optional: None,
                    required: "present".to_string(),
                }
            );
        }
    }

    #[test]
    fn empty_yaml_parses_as_an_empty_map() {
        for input in [
            "",
            "  \n\t",
            "# comment",
            "  # comment\n\n# another",
            "---",
            "--- # comment",
            "%YAML 1.2\n---",
            "---\n...",
            "\u{feff}",
        ] {
            assert_eq!(
                parse_config_value(input, Format::Yaml).unwrap(),
                Value::Object(ConfigMap::new())
            );
        }
    }

    #[test]
    fn explicit_top_level_yaml_null_is_rejected() {
        for input in ["null", "~", "--- null", "---\nnull"] {
            assert!(deserialize_config::<ConfigMap>(input, Format::Yaml).is_err());
        }
    }

    #[test]
    fn deserialization_errors_include_the_value_path() {
        let error = deserialize_config::<NestedValue>(
            indoc! {"
                outer:
                  count: not-a-number
            "},
            Format::Yaml,
        )
        .unwrap_err();

        assert!(
            error
                .first()
                .is_some_and(|error| error.contains("outer.count")),
            "expected the error to contain the nested value path, got {error:?}"
        );
    }

    #[test]
    fn explicit_null_is_preserved() {
        let json = parse_config_value(r#"{"value": null}"#, Format::Json).unwrap();
        let yaml = parse_config_value("value: null", Format::Yaml).unwrap();

        assert_eq!(json, json!({ "value": null }));
        assert_eq!(yaml, json!({ "value": null }));
    }

    #[test]
    fn explicit_null_deserializes_as_none_for_optional_fields() {
        for (input, format) in [
            (r#"{"optional": null, "required": "present"}"#, Format::Json),
            ("optional: null\nrequired: present", Format::Yaml),
        ] {
            assert_eq!(
                deserialize_config::<OptionalValue>(input, format).unwrap(),
                OptionalValue {
                    optional: None,
                    required: "present".to_string(),
                }
            );
        }
    }

    #[test]
    fn explicit_null_is_rejected_for_required_fields() {
        for (input, format) in [
            (r#"{"required": null}"#, Format::Json),
            ("required: null", Format::Yaml),
        ] {
            assert!(deserialize_config::<OptionalValue>(input, format).is_err());
        }
    }

    #[test]
    fn defaulted_optional_field_preserves_missing_null_and_string_semantics() {
        for (input, format, expected) in [
            ("", Format::Toml, Some("default")),
            ("", Format::Yaml, Some("default")),
            ("{}", Format::Json, Some("default")),
            (
                indoc! {r#"
                    value = "custom"
                "#},
                Format::Toml,
                Some("custom"),
            ),
            (
                indoc! {"
                    value: custom
                "},
                Format::Yaml,
                Some("custom"),
            ),
            (
                indoc! {r#"
                    {
                      "value": "custom"
                    }
                "#},
                Format::Json,
                Some("custom"),
            ),
            (
                indoc! {r#"
                    value = ""
                "#},
                Format::Toml,
                Some(""),
            ),
            (
                indoc! {r#"
                    value: ""
                "#},
                Format::Yaml,
                Some(""),
            ),
            (
                indoc! {r#"
                    {
                      "value": ""
                    }
                "#},
                Format::Json,
                Some(""),
            ),
            (
                indoc! {r#"
                    value = "null"
                "#},
                Format::Toml,
                Some("null"),
            ),
            (
                indoc! {r#"
                    value: "null"
                "#},
                Format::Yaml,
                Some("null"),
            ),
            (
                indoc! {r#"
                    {
                      "value": "null"
                    }
                "#},
                Format::Json,
                Some("null"),
            ),
            (
                indoc! {"
                    value: null
                "},
                Format::Yaml,
                None,
            ),
            (
                indoc! {r#"
                    {
                      "value": null
                    }
                "#},
                Format::Json,
                None,
            ),
        ] {
            let config = deserialize_config::<DefaultedOptionalValue>(input, format).unwrap();

            assert_eq!(config.value.as_deref(), expected);
        }
    }

    #[test]
    fn yaml_merge_keys_are_applied() {
        let value: Value = deserialize_config(
            "defaults: &defaults\n  codec: json\nencoding:\n  <<: *defaults",
            Format::Yaml,
        )
        .unwrap();

        assert_eq!(
            value,
            json!({
                "defaults": { "codec": "json" },
                "encoding": { "codec": "json" }
            })
        );
    }

    #[test]
    fn toml_datetime_deserializes_as_a_string() {
        let value: StringValue =
            deserialize_config("value = 1979-05-27T07:32:00Z", Format::Toml).unwrap();

        assert_eq!(
            value,
            StringValue {
                value: "1979-05-27T07:32:00Z".to_string()
            }
        );
    }

    #[test]
    fn rejects_large_json_and_yaml_integers() {
        for (input, format) in [
            (r#"{"value": 9223372036854775808}"#, Format::Json),
            ("value: 9223372036854775808", Format::Yaml),
        ] {
            assert_eq!(
                deserialize_config::<ConfigMap>(input, format).unwrap_err(),
                vec![super::LARGE_INTEGER_ERROR.to_string()]
            );
        }
    }

    #[test]
    fn rejects_non_finite_toml_and_yaml_floats() {
        for (input, format) in [("value = inf", Format::Toml), ("value: .nan", Format::Yaml)] {
            assert_eq!(
                deserialize_config::<ConfigMap>(input, format).unwrap_err(),
                vec![super::NON_FINITE_FLOAT_ERROR.to_string()]
            );
        }
    }

    #[test]
    fn merge_replaces_same_type_scalars() {
        assert_eq!(
            merge_values(json!("first"), json!("second")).unwrap(),
            json!("second")
        );
    }

    #[test]
    fn deferred_scalar_conflicts_preserve_merge_semantics() {
        struct Case {
            name: &'static str,
            fragments: Vec<Value>,
            expected: Result<Value, &'static str>,
        }

        let schema = json!({
            "type": "object",
            "properties": {
                "enabled": {"type": "boolean"},
                "a/b~c": {"type": "integer"},
                "items": {"type": "array", "items": {"type": "integer"}},
                "untyped": {},
                "string": {"type": "string"},
                "choice": {"enum": ["final"]}
            }
        });
        for case in [
            Case {
                name: "the later interpolated scalar wins",
                fragments: vec![json!({"enabled": false}), json!({"enabled": "true"})],
                expected: Ok(json!({"enabled": true})),
            },
            Case {
                name: "the later literal scalar wins",
                fragments: vec![json!({"enabled": "true"}), json!({"enabled": false})],
                expected: Ok(json!({"enabled": false})),
            },
            Case {
                name: "overwritten invalid values are still checked",
                fragments: vec![json!({"enabled": "invalid"}), json!({"enabled": true})],
                expected: Err("Expected boolean at 'enabled'"),
            },
            Case {
                name: "an invalid intermediate override is still checked",
                fragments: vec![
                    json!({"enabled": true}),
                    json!({"enabled": "invalid"}),
                    json!({"enabled": false}),
                ],
                expected: Err("Expected boolean at 'enabled'"),
            },
            Case {
                name: "a later same-type string cannot hide an invalid conflict",
                fragments: vec![
                    json!({"enabled": true}),
                    json!({"enabled": "invalid"}),
                    json!({"enabled": "false"}),
                ],
                expected: Err("Expected boolean at 'enabled'"),
            },
            Case {
                name: "array concatenation and pointer escaping are preserved",
                fragments: vec![
                    json!({"a/b~c": "42", "items": [1]}),
                    json!({"a/b~c": 43, "items": [2]}),
                ],
                expected: Ok(json!({"a/b~c": 43, "items": [1, 2]})),
            },
            Case {
                name: "a schema must actually make the types compatible",
                fragments: vec![json!({"untyped": "true"}), json!({"untyped": true})],
                expected: Err("Incompatible types"),
            },
            Case {
                name: "scalar coercion cannot turn overrides into array concatenation",
                fragments: vec![json!({"items": "1"}), json!({"items": 2})],
                expected: Err("Incompatible types"),
            },
            Case {
                name: "ordinary string overrides do not validate overwritten values",
                fragments: vec![json!({"choice": "obsolete"}), json!({"choice": "final"})],
                expected: Ok(json!({"choice": "final"})),
            },
            Case {
                name: "native scalars cannot become string-field overrides",
                fragments: vec![json!({"string": "42"}), json!({"string": 43})],
                expected: Err("Incompatible types"),
            },
        ] {
            let maps = case
                .fragments
                .into_iter()
                .map(|value| serde_json::from_value::<ConfigMap>(value).unwrap());
            let result = merge_maps_deferred(maps).and_then(|(merged, conflicts)| {
                resolve_merge_conflicts(merged, conflicts, |value| {
                    ValueCoercer::new(&schema)
                        .coerce(value)
                        .map_err(|error| vec![error.to_string()])
                })
            });
            match case.expected {
                Ok(expected) => assert_eq!(
                    Value::Object(
                        result.unwrap_or_else(|errors| panic!("{}: {errors:?}", case.name))
                    ),
                    expected,
                    "{}",
                    case.name
                ),
                Err(expected_error) => {
                    let errors = result.unwrap_err();
                    assert!(
                        errors.iter().any(|error| error.contains(expected_error)),
                        "{}: {errors:?}",
                        case.name
                    );
                }
            }
        }
    }

    #[test]
    fn merge_concatenates_arrays_in_order() {
        assert_eq!(
            merge_values(json!([1, 2]), json!([3, 4])).unwrap(),
            json!([1, 2, 3, 4])
        );
    }

    #[test]
    fn merge_recurses_into_maps() {
        assert_eq!(
            merge_values(
                json!({
                    "nested": {
                        "first": 1,
                        "replaced": "old"
                    }
                }),
                json!({
                    "nested": {
                        "second": 2,
                        "replaced": "new"
                    }
                }),
            )
            .unwrap(),
            json!({
                "nested": {
                    "first": 1,
                    "second": 2,
                    "replaced": "new"
                }
            })
        );
    }

    #[test]
    fn merge_reports_nested_type_conflicts() {
        let error = merge_values(
            json!({ "nested": { "value": "string" } }),
            json!({ "nested": { "value": 1 } }),
        )
        .unwrap_err();

        assert_eq!(
            error,
            vec![
                r#"Incompatible types at path "$.nested.value", expected "string" received "integer"."#
                    .to_string()
            ]
        );
    }

    #[test]
    fn merge_treats_integers_and_floats_as_different_types() {
        let error = merge_values(json!(1), json!(1.0)).unwrap_err();

        assert_eq!(
            error,
            vec![
                r#"Incompatible types at path "$", expected "integer" received "float"."#
                    .to_string()
            ]
        );
    }

    #[test]
    fn merge_only_accepts_null_with_null() {
        assert_eq!(merge_values(Value::Null, Value::Null).unwrap(), Value::Null);

        for (first, second, expected, received) in [
            (Value::Null, json!(1), "null", "integer"),
            (json!(1), Value::Null, "integer", "null"),
        ] {
            assert_eq!(
                merge_values(first, second).unwrap_err(),
                vec![format!(
                    r#"Incompatible types at path "$", expected "{expected}" received "{received}"."#
                )]
            );
        }
    }
}
