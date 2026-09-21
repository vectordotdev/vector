use super::{Error, ValueCoercer};
use serde_json::json;

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
