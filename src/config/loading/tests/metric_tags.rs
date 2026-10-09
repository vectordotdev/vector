use std::collections::HashMap;

use serde_json::{Value, json};

use super::with_absent_env_var;
use crate::config::{Format, loading::ConfigBuilderLoader};

fn load_metric_tag(tag: &Value, secrets: HashMap<String, String>) -> Result<Value, Vec<String>> {
    let input = json!({"tests": [{
        "name": "metric tag input",
        "inputs": [{
            "insert_at": "unused",
            "type": "metric",
            "metric": {
                "name": "count", "kind": "absolute",
                "tags": {"tag": tag}, "counter": {"value": 1}
            }
        }]
    }]})
    .to_string();
    let input = with_absent_env_var(&input);
    ConfigBuilderLoader::default()
        .interpolate_env(true)
        .secrets(secrets)
        .load_from_input(input.as_bytes(), Format::Json)
        .map(|builder| {
            let metric = builder.tests[0].inputs[0].metric.as_ref().unwrap();
            serde_json::to_value(metric.tags().unwrap()).unwrap()["tag"].clone()
        })
}

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "Keep the acceptance table with its shared assertions"
)]
fn native_metric_tags_keep_serde_acceptance_and_normalization() {
    struct Case {
        name: &'static str,
        input: Value,
        expected: Option<Value>,
    }

    for case in [
        Case {
            name: "single string",
            input: json!("value"),
            expected: Some(json!("value")),
        },
        Case {
            name: "empty string",
            input: json!(""),
            expected: Some(json!("")),
        },
        Case {
            name: "null-looking string",
            input: json!("null"),
            expected: Some(json!("null")),
        },
        Case {
            name: "number-looking string",
            input: json!("42"),
            expected: Some(json!("42")),
        },
        Case {
            name: "boolean-looking string",
            input: json!("false"),
            expected: Some(json!("false")),
        },
        Case {
            name: "bare tag",
            input: Value::Null,
            expected: Some(Value::Null),
        },
        Case {
            name: "empty array",
            input: json!([]),
            expected: Some(json!([])),
        },
        Case {
            name: "single bare tag array",
            input: json!([null]),
            expected: Some(Value::Null),
        },
        Case {
            name: "single string array",
            input: json!(["value"]),
            expected: Some(json!("value")),
        },
        Case {
            name: "duplicates preserve last occurrence order",
            input: json!(["first", "second", "first"]),
            expected: Some(json!(["second", "first"])),
        },
        Case {
            name: "duplicate bare tags and strings",
            input: json!([null, "first", null, "second"]),
            expected: Some(json!(["first", null, "second"])),
        },
        Case {
            name: "string literals within an array",
            input: json!(["", "null", "42", "false"]),
            expected: Some(json!(["", "null", "42", "false"])),
        },
        Case {
            name: "integer",
            input: json!(42),
            expected: None,
        },
        Case {
            name: "float",
            input: json!(1.5),
            expected: None,
        },
        Case {
            name: "true",
            input: json!(true),
            expected: None,
        },
        Case {
            name: "false",
            input: json!(false),
            expected: None,
        },
        Case {
            name: "internal Single representation",
            input: json!({"Single": "value"}),
            expected: None,
        },
        Case {
            name: "internal Set representation",
            input: json!({"Set": ["first", "second"]}),
            expected: None,
        },
        Case {
            name: "empty object",
            input: json!({}),
            expected: None,
        },
        Case {
            name: "integer array member",
            input: json!(["value", 42]),
            expected: None,
        },
        Case {
            name: "boolean array member",
            input: json!(["value", true]),
            expected: None,
        },
        Case {
            name: "object array member",
            input: json!([{"Single": "value"}]),
            expected: None,
        },
        Case {
            name: "nested array member",
            input: json!([["value"]]),
            expected: None,
        },
    ] {
        let result = load_metric_tag(&case.input, HashMap::new());
        if let Some(expected) = case.expected {
            assert_eq!(
                result.unwrap_or_else(|errors| panic!("{}: {errors:?}", case.name)),
                expected,
                "{}",
                case.name
            );
        } else {
            assert!(result.is_err(), "{}: {result:?}", case.name);
        }
    }
}

#[test]
fn metric_tag_interpolation_preserves_literal_strings() {
    struct Case {
        name: &'static str,
        value: &'static str,
        secret: &'static str,
    }

    for case in [
        Case {
            name: "ordinary text",
            value: "tag value",
            secret: "tag value",
        },
        Case {
            name: "empty text",
            value: "",
            secret: "",
        },
        Case {
            name: "null text",
            value: "null",
            secret: "null",
        },
        Case {
            name: "integer text",
            value: "42",
            secret: "42",
        },
        Case {
            name: "boolean text",
            value: "false",
            secret: "false",
        },
        Case {
            name: "unescaped quotes in a secret remain literal",
            value: r#"["first", "second"]"#,
            secret: r#"["first", "second"]"#,
        },
        Case {
            name: "JSON escapes in a secret remain literal",
            value: r#"["first", "second"]"#,
            secret: r#"[\"first\", \"second\"]"#,
        },
    ] {
        let environment = format!("${{VECTOR_TEST_COMPAT_UNSET:-{}}}", case.value);
        for (source, input, value) in [
            ("environment", environment, case.value),
            ("secret", "SECRET[backend.tag]".to_owned(), case.secret),
        ] {
            for (position, input, expected) in [
                ("scalar", json!(input), json!(value)),
                (
                    "array member",
                    json!([input, null, "neighbor"]),
                    json!([value, null, "neighbor"]),
                ),
            ] {
                let result = load_metric_tag(
                    &input,
                    HashMap::from([("backend.tag".into(), case.secret.into())]),
                );
                assert_eq!(
                    result.unwrap_or_else(|errors| panic!(
                        "{} / {source} / {position}: {errors:?}",
                        case.name
                    )),
                    expected,
                    "{} / {source} / {position}",
                    case.name
                );
            }
        }
    }
}
