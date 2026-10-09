use std::collections::HashMap;

use serde_json::{Value, json};
use vector_lib::event::MetricTags;

use super::ConfigBuilderLoader;
use crate::config::Format;

fn load_tag(value: &Value, secrets: HashMap<String, String>) -> Result<MetricTags, Vec<String>> {
    let input = json!({"tests": [{
        "name": "metric tag input",
        "inputs": [{
            "insert_at": "unused",
            "type": "metric",
            "metric": {
                "name": "count",
                "kind": "absolute",
                "tags": {"tag": value},
                "counter": {"value": 1}
            }
        }]
    }]})
    .to_string();

    ConfigBuilderLoader::default()
        .interpolate_env(true)
        .secrets(secrets)
        .load_from_input(input.as_bytes(), Format::Json)
        .map(|builder| {
            builder.tests[0].inputs[0]
                .metric
                .as_ref()
                .expect("the test input contains a metric")
                .tags()
                .expect("the metric contains tags")
                .clone()
        })
}

#[test]
fn native_metric_tags_keep_serde_acceptance_and_normalization() {
    struct Case {
        name: &'static str,
        input: Value,
        accepted: bool,
    }

    for case in [
        Case {
            name: "single string",
            input: json!("value"),
            accepted: true,
        },
        Case {
            name: "empty string",
            input: json!(""),
            accepted: true,
        },
        Case {
            name: "literal null remains a string",
            input: json!("null"),
            accepted: true,
        },
        Case {
            name: "literal number remains a string",
            input: json!("42"),
            accepted: true,
        },
        Case {
            name: "literal boolean remains a string",
            input: json!("false"),
            accepted: true,
        },
        Case {
            name: "bare tag",
            input: Value::Null,
            accepted: true,
        },
        Case {
            name: "empty array",
            input: json!([]),
            accepted: true,
        },
        Case {
            name: "single bare tag array",
            input: json!([null]),
            accepted: true,
        },
        Case {
            name: "single string array",
            input: json!(["value"]),
            accepted: true,
        },
        Case {
            name: "duplicate values preserve last occurrence order",
            input: json!(["first", "second", "first"]),
            accepted: true,
        },
        Case {
            name: "duplicate bare tags and strings",
            input: json!([null, "first", null, "second"]),
            accepted: true,
        },
        Case {
            name: "string literals within an array",
            input: json!(["", "null", "42", "false"]),
            accepted: true,
        },
        Case {
            name: "native integer",
            input: json!(42),
            accepted: false,
        },
        Case {
            name: "native float",
            input: json!(1.5),
            accepted: false,
        },
        Case {
            name: "native true",
            input: json!(true),
            accepted: false,
        },
        Case {
            name: "native false",
            input: json!(false),
            accepted: false,
        },
        Case {
            name: "internal Single representation",
            input: json!({"Single": "value"}),
            accepted: false,
        },
        Case {
            name: "internal Set representation",
            input: json!({"Set": ["first", "second"]}),
            accepted: false,
        },
        Case {
            name: "empty object",
            input: json!({}),
            accepted: false,
        },
        Case {
            name: "integer array member",
            input: json!(["value", 42]),
            accepted: false,
        },
        Case {
            name: "boolean array member",
            input: json!(["value", true]),
            accepted: false,
        },
        Case {
            name: "object array member",
            input: json!([{"Single": "value"}]),
            accepted: false,
        },
        Case {
            name: "nested array member",
            input: json!([["value"]]),
            accepted: false,
        },
    ] {
        let expected = serde_json::from_value::<MetricTags>(json!({"tag": case.input}));
        assert_eq!(expected.is_ok(), case.accepted, "{}", case.name);

        let loaded = load_tag(&case.input, HashMap::new());
        if let Ok(expected) = expected {
            let loaded = loaded.unwrap_or_else(|errors| panic!("{}: {errors:?}", case.name));
            assert_eq!(loaded, expected, "{}", case.name);
            assert_eq!(
                serde_json::to_value(loaded).unwrap(),
                serde_json::to_value(expected).unwrap(),
                "{}",
                case.name
            );
        } else {
            assert!(loaded.is_err(), "{}", case.name);
        }
    }
}

#[test]
fn interpolated_metric_tag_strings_keep_their_text() {
    struct Case {
        name: &'static str,
        value: &'static str,
    }

    for case in [
        Case {
            name: "ordinary text",
            value: "tag value",
        },
        Case {
            name: "empty text",
            value: "",
        },
        Case {
            name: "null text",
            value: "null",
        },
        Case {
            name: "integer text",
            value: "42",
        },
        Case {
            name: "boolean text",
            value: "false",
        },
        Case {
            name: "array-looking text",
            value: "[\"first\", \"second\"]",
        },
    ] {
        let environment = format!("${{VECTOR_TEST_METRIC_TAG_UNSET:-{}}}", case.value);
        for (source, input) in [
            ("environment", json!(environment)),
            ("secret", json!("SECRET[backend.tag]")),
        ] {
            for (position, input, expected) in [
                ("scalar", input.clone(), json!(case.value)),
                (
                    "array member",
                    json!([input, null, "neighbor"]),
                    json!([case.value, null, "neighbor"]),
                ),
            ] {
                let secrets = HashMap::from([("backend.tag".into(), case.value.into())]);
                let loaded = load_tag(&input, secrets).unwrap_or_else(|errors| {
                    panic!("{} from {source} as {position}: {errors:?}", case.name)
                });
                let expected: MetricTags =
                    serde_json::from_value(json!({"tag": expected})).unwrap();
                assert_eq!(
                    loaded, expected,
                    "{} from {source} as {position}",
                    case.name
                );
                assert_eq!(
                    serde_json::to_value(loaded).unwrap(),
                    serde_json::to_value(expected).unwrap(),
                    "{} from {source} as {position}",
                    case.name
                );
            }
        }
    }
}
