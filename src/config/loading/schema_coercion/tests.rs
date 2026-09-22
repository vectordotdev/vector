use super::{Error, ValueCoercer};
use serde_json::json;

/// Object alternatives with different required fields.
#[vector_config::configurable_component]
#[derive(Debug, PartialEq)]
#[serde(untagged)]
enum RequiredFields {
    /// The older settings form.
    V1 {
        /// Required database name.
        database: String,
    },
    /// The newer settings form.
    V2 {
        /// Required organization name, including legacy spellings.
        #[serde(rename = "org", alias = "organization", alias = "tenant")]
        organization: String,
    },
}

#[test]
fn generated_object_union_checks_required_fields_and_aliases() {
    let schema = serde_json::to_value(
        vector_config::schema::generate_root_schema::<RequiredFields>().unwrap(),
    )
    .unwrap();
    for key in ["org", "organization", "tenant"] {
        let mut value = json!({key: 42});
        ValueCoercer::new(&schema).coerce(&mut value).unwrap();
        assert_eq!(value, json!({key: "42"}));
        assert_eq!(
            serde_json::from_value::<RequiredFields>(value).unwrap(),
            RequiredFields::V2 {
                organization: "42".into()
            }
        );
    }
    assert!(ValueCoercer::new(&schema).coerce(&mut json!({})).is_err());
}

#[test]
fn failed_nested_one_of_allows_the_next_any_of_candidate() {
    let schema = json!({"anyOf": [
        {"oneOf": [{"type": "object", "required": ["name"],
            "properties": {"name": {"type": "string"}}}]},
        {"type": "object", "required": ["count"],
            "properties": {"count": {"type": "integer"}}}
    ]});
    let mut value = json!({"count": "42"});
    ValueCoercer::new(&schema).coerce(&mut value).unwrap();
    assert_eq!(value, json!({"count": 42}));
    assert!(
        ValueCoercer::new(&schema)
            .coerce(&mut json!({"count": "bad"}))
            .is_err()
    );
}

/// An optional flattened tagged setting.
#[vector_config::configurable_component]
#[derive(Debug)]
struct OptionalMode {
    #[serde(flatten)]
    mode: Option<Mode>,
}

/// A mode whose payload needs coercion.
#[vector_config::configurable_component]
#[derive(Debug)]
#[serde(tag = "mode", rename_all = "snake_case")]
enum Mode {
    /// Counted mode.
    Counted {
        /// Number of items.
        count: usize,
    },
}

#[test]
fn generated_optional_flattened_enum_checks_tag_absence() {
    let schema = serde_json::to_value(
        vector_config::schema::generate_root_schema::<OptionalMode>().unwrap(),
    )
    .unwrap();
    let mut absent = json!({});
    ValueCoercer::new(&schema).coerce(&mut absent).unwrap();
    assert!(
        serde_json::from_value::<OptionalMode>(absent)
            .unwrap()
            .mode
            .is_none()
    );
    let mut value = json!({"mode": "counted", "count": "42"});
    ValueCoercer::new(&schema).coerce(&mut value).unwrap();
    assert_eq!(value["count"], json!(42));
    assert!(matches!(
        serde_json::from_value::<OptionalMode>(value).unwrap().mode,
        Some(Mode::Counted { count: 42 })
    ));
    for mut invalid in [
        json!({"mode": "counted", "count": "bad"}),
        json!({"mode": "unknown", "count": "42"}),
    ] {
        assert!(ValueCoercer::new(&schema).coerce(&mut invalid).is_err());
    }
}

#[test]
fn generated_nonzero_number_checks_negation_after_coercion() {
    let schema = serde_json::to_value(
        vector_config::schema::generate_root_schema::<std::num::NonZeroI32>().unwrap(),
    )
    .unwrap();
    assert!(ValueCoercer::new(&schema).coerce(&mut json!("0")).is_err());
    let mut value = json!("42");
    ValueCoercer::new(&schema).coerce(&mut value).unwrap();
    assert_eq!(value, json!(42));
}

#[test]
fn negated_null_does_not_coerce_a_string_to_null() {
    let schema = json!({"not": {"type": "null"}});
    let mut value = json!("null");
    ValueCoercer::new(&schema).coerce(&mut value).unwrap();
    assert_eq!(value, json!("null"));
    assert!(ValueCoercer::new(&schema).coerce(&mut json!(null)).is_err());
}

#[test]
fn structured_values_are_not_coerced_to_strings() {
    let schema = json!({"type": "string"});
    for mut value in [json!({"source": "bad"}), json!(["bad"])] {
        let original = value.clone();
        assert!(matches!(
            ValueCoercer::new(&schema).coerce(&mut value),
            Err(Error::ExpectedString { .. })
        ));
        assert_eq!(value, original);
    }
}

#[test]
fn failing_object_union_does_not_fall_back_to_json_string() {
    let schema = json!({"properties": {"condition": {"anyOf": [
        {"type": "string"},
        {"type": "object", "properties": {"count": {"type": "integer"}}}
    ]}}});
    let mut value = json!({"condition": {"count": "bad"}});
    let original = value.clone();
    let error = ValueCoercer::new(&schema).coerce(&mut value).unwrap_err();
    assert!(matches!(error, Error::Coerce { ref path, .. } if path == "condition"));
    assert_eq!(value, original);
}

#[test]
fn nullable_array_keeps_null_when_item_union_rejects_it() {
    let schema = json!({
        "type": ["array", "null"],
        "items": {"anyOf": [{"type": "string"}, {"type": "object"}]}
    });
    let mut value = json!(null);
    ValueCoercer::new(&schema).coerce(&mut value).unwrap();
    assert_eq!(value, json!(null));
}

#[test]
fn rejected_union_is_not_a_successful_noop() {
    let schema = json!({"anyOf": [{"type": "string"}, {"type": "object"}]});
    let mut value = json!(null);
    assert!(ValueCoercer::new(&schema).coerce(&mut value).is_err());
    assert_eq!(value, json!(null));
}

#[test]
fn generated_test_output_preserves_null_conditions() {
    let schema = serde_json::to_value(
        vector_config::schema::generate_root_schema::<crate::config::TestOutput<String>>().unwrap(),
    )
    .unwrap();
    let mut value = json!({"extract_from": ["transform"], "conditions": null});
    ValueCoercer::new(&schema).coerce(&mut value).unwrap();
    assert_eq!(value["conditions"], json!(null));
    let output: crate::config::TestOutput<String> = serde_json::from_value(value).unwrap();
    assert!(output.conditions.is_none());
}

#[test]
fn scalar_coercions() {
    for (kind, input, expected) in [
        ("integer", json!("42"), json!(42)),
        ("integer", json!(u64::MAX.to_string()), json!(u64::MAX)),
        ("integer", json!(i64::MIN.to_string()), json!(i64::MIN)),
        ("number", json!("1.5"), json!(1.5)),
        ("boolean", json!("true"), json!(true)),
        ("null", json!(" null "), json!(null)),
        ("string", json!(false), json!("false")),
    ] {
        let schema = json!({"type": kind});
        let mut value = input;
        ValueCoercer::new(&schema).coerce(&mut value).unwrap();
        assert_eq!(value, expected, "{kind}");
    }
}

#[test]
fn invalid_scalars_do_not_saturate_or_become_nonfinite() {
    for (kind, input) in [
        ("integer", json!("18446744073709551616")),
        ("integer", json!("-9223372036854775809")),
        ("integer", json!(9223372036854775808.0_f64)),
        ("integer", json!(1.5)),
        ("number", json!("NaN")),
        ("number", json!("inf")),
        ("boolean", json!("yes")),
        ("string", json!(null)),
    ] {
        let schema = json!({"type": kind});
        let mut value = input.clone();
        assert!(
            ValueCoercer::new(&schema).coerce(&mut value).is_err(),
            "{input}"
        );
        assert_eq!(value, input);
    }
}

#[test]
fn nested_reference_errors_have_paths_and_coercer_is_reusable() {
    let schema = json!({
        "type": "object",
        "properties": {"counts": {"type": "array", "items": {"$ref": "#/definitions/count"}}},
        "definitions": {"count": {"type": "integer"}}
    });
    let mut coercer = ValueCoercer::new(&schema);
    let error = coercer
        .coerce(&mut json!({"counts": ["1", "bad"]}))
        .unwrap_err();
    assert!(matches!(error, Error::ExpectedInteger { ref path, .. } if path == "counts.1"));
    let mut value = json!({"counts": ["2"]});
    coercer.coerce(&mut value).unwrap();
    assert_eq!(value, json!({"counts": [2]}));
    let error = coercer.coerce(&mut json!({"counts": ["bad"]})).unwrap_err();
    assert!(matches!(error, Error::ExpectedInteger { ref path, .. } if path == "counts.0"));
}

#[test]
fn reference_errors_are_explicit() {
    for (reference, missing) in [
        ("#/definitions/missing", true),
        ("https://example.com/schema", false),
    ] {
        let schema = json!({"properties": {"value": {"$ref": reference}}});
        let error = ValueCoercer::new(&schema)
            .coerce(&mut json!({"value": "1"}))
            .unwrap_err();
        match error {
            Error::SchemaReferenceNotFound { path, .. } if missing => assert_eq!(path, "value"),
            Error::UnsupportedSchemaReference { path, .. } if !missing => assert_eq!(path, "value"),
            error => panic!("unexpected error: {error}"),
        }
    }
}

#[test]
fn arrays_and_additional_properties() {
    let schema = json!({
        "type": "object",
        "properties": {"tuple": {"type": "array", "items": [{"type": "integer"}, {"type": "boolean"}], "additionalItems": false}},
        "additionalProperties": {"type": "number"}
    });
    let mut value = json!({"tuple": ["2", "false"], "extra": "1.5"});
    ValueCoercer::new(&schema).coerce(&mut value).unwrap();
    assert_eq!(value, json!({"tuple": [2, false], "extra": 1.5}));
    let error = ValueCoercer::new(&schema)
        .coerce(&mut json!({"tuple": ["2", "false", 3]}))
        .unwrap_err();
    assert!(
        matches!(error, Error::UnexpectedArrayElement { ref path, index: 2 } if path == "tuple")
    );
    let schema = json!({"type": "array", "items": {"type": "integer"}});
    let mut value = json!("2");
    ValueCoercer::new(&schema).coerce(&mut value).unwrap();
    assert_eq!(value, json!([2]));
}

#[test]
fn enum_const_and_boolean_schemas() {
    for (schema, mut value, expected) in [
        (json!({"enum": [1, 2]}), json!("2"), json!(2)),
        (json!({"const": false}), json!("false"), json!(false)),
        (
            json!(true),
            json!({"untouched": "2"}),
            json!({"untouched": "2"}),
        ),
    ] {
        ValueCoercer::new(&schema).coerce(&mut value).unwrap();
        assert_eq!(value, expected);
    }
    for schema in [
        json!({"enum": [1, 2]}),
        json!({"const": false}),
        json!(false),
    ] {
        assert!(
            ValueCoercer::new(&schema)
                .coerce(&mut json!("bad"))
                .is_err()
        );
    }
}

#[test]
fn tagged_union_preserves_specific_error_path() {
    let schema = json!({"properties": {"source": {"oneOf": [
        {"properties": {"type": {"const": "demo"}, "count": {"type": "integer"}}},
        {"properties": {"type": {"const": "other"}, "enabled": {"type": "boolean"}}}
    ]}}});
    let error = ValueCoercer::new(&schema)
        .coerce(&mut json!({"source": {"type": "demo", "count": "bad"}}))
        .unwrap_err();
    assert!(matches!(error, Error::ExpectedInteger { ref path, .. } if path == "source.count"));
}

#[cfg(test)]
mod union_tests {
    use super::super::ValueCoercer;
    use serde_json::json;

    fn untagged_string_or_map_schema() -> serde_json::Value {
        json!({
            "anyOf": [
                { "type": "string" },
                { "$ref": "#/definitions/ConditionMap" }
            ],
            "definitions": {
                "ConditionMap": {
                    "oneOf": [{
                        "type": "object",
                        "properties": {
                            "type": { "const": "vrl" },
                            "source": { "type": "string" }
                        }
                    }]
                }
            }
        })
    }

    #[test]
    fn any_of_prefers_a_structurally_compatible_object_variant() {
        let schema = untagged_string_or_map_schema();
        let mut input = json!({
            "type": "vrl",
            "source": ".status_code != 200"
        });

        ValueCoercer::new(&schema).coerce(&mut input).unwrap();

        assert_eq!(
            input,
            json!({
                "type": "vrl",
                "source": ".status_code != 200"
            })
        );
    }

    #[test]
    fn any_of_preserves_a_structurally_compatible_string_variant() {
        let schema = untagged_string_or_map_schema();
        let mut input = json!(".status_code != 200");

        ValueCoercer::new(&schema).coerce(&mut input).unwrap();

        assert_eq!(input, json!(".status_code != 200"));
    }
}

#[cfg(all(test, feature = "sources-demo_logs",))]
mod test {
    use crate::config::ConfigBuilder;
    use crate::config::loading::schema_coercion::ValueCoercer;
    use serde_json::json;
    use vector_config::schema::generate_root_schema;

    #[test]
    fn generated_demo_logs_alias_is_coerced_and_deserializes() {
        let schema =
            serde_json::to_value(generate_root_schema::<ConfigBuilder>().unwrap()).unwrap();
        let mut value = json!({"sources": {"demo": {
            "type": "demo_logs", "format": "json", "batch_interval": "1.5"
        }}});
        ValueCoercer::new(&schema).coerce(&mut value).unwrap();
        assert_eq!(value["sources"]["demo"]["batch_interval"], json!(1.5));
        serde_json::from_value::<ConfigBuilder>(value).unwrap();
    }

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
        ValueCoercer::new(&demo_logs_schema)
            .coerce(&mut input)
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
        // Unknown fields remain non-fatal; serde validates them downstream.
        let mut input = json!({
            "sources": {
                "source0": {
                    "type": "demo_logs",
                    "count": 100,
                    "totally_unknown_field": "oops",
                    "format": "json",
                }
            }
        });

        let schema =
            serde_json::to_value(generate_root_schema::<ConfigBuilder>().unwrap()).unwrap();
        let result = ValueCoercer::new(&schema).coerce(&mut input);

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
        let result = ValueCoercer::new(&schema).coerce(&mut input);

        assert!(
            result.is_ok(),
            "unknown component type should pass through coercion, got: {result:?}"
        );
    }
}
