use std::collections::HashMap;

use indoc::indoc;
use serde_json::{Value, json};

use super::{
    ConfigBuilderLoader, SecretBackendLoader, SourceLoader, loader_from_input, loader_from_paths,
};
use crate::config::{ConfigPath, Format};

struct ScalarCase {
    name: &'static str,
    format: Format,
    input: &'static str,
    expected: Value,
}

#[test]
fn interpolated_scalars_use_the_component_schema() {
    for case in [
        ScalarCase {
            name: "YAML plain placeholders and a field alias",
            format: Format::Yaml,
            input: indoc! {r#"
                sources:
                  demo:
                    type: demo_logs
                    format: json
                    count: ${VECTOR_TEST_PARSE_FIRST_COUNT:-42}
                    batch_interval: ${VECTOR_TEST_PARSE_FIRST_INTERVAL:-1.5}
                    log_namespace: ${VECTOR_TEST_PARSE_FIRST_BOOL:-true}
            "#},
            expected: json!({"count": 42, "interval": 1.5, "log_namespace": true}),
        },
        ScalarCase {
            name: "TOML quoted placeholders",
            format: Format::Toml,
            input: indoc! {r#"
                [sources.demo]
                type = 'demo_logs'
                format = 'json'
                count = '${VECTOR_TEST_PARSE_FIRST_COUNT:-42}'
                interval = '${VECTOR_TEST_PARSE_FIRST_INTERVAL:-1.5}'
                log_namespace = '${VECTOR_TEST_PARSE_FIRST_BOOL:-true}'
            "#},
            expected: json!({"count": 42, "interval": 1.5, "log_namespace": true}),
        },
        ScalarCase {
            name: "JSON quoted placeholders",
            format: Format::Json,
            input: indoc! {r#"
                {"sources": {"demo": {
                    "type": "demo_logs",
                    "format": "json",
                    "count": "${VECTOR_TEST_PARSE_FIRST_COUNT:-42}",
                    "interval": "${VECTOR_TEST_PARSE_FIRST_INTERVAL:-1.5}",
                    "log_namespace": "${VECTOR_TEST_PARSE_FIRST_BOOL:-true}"
                }}}
            "#},
            expected: json!({"count": 42, "interval": 1.5, "log_namespace": true}),
        },
    ] {
        let builder = ConfigBuilderLoader::default()
            .interpolate_env(true)
            .load_from_input(case.input.as_bytes(), case.format)
            .unwrap_or_else(|error| panic!("{}: {error:?}", case.name));
        let value = serde_json::to_value(builder).unwrap();
        let source = &value["sources"]["demo"];
        assert_eq!(
            json!({
                "count": source["count"],
                "interval": source["interval"],
                "log_namespace": source["log_namespace"]
            }),
            case.expected,
            "{}",
            case.name
        );
    }
}

#[test]
fn comments_are_not_interpolated_and_disabled_interpolation_preserves_strings() {
    let input = indoc! {r#"
        # ${VECTOR_TEST_PARSE_FIRST_UNSET:?must not be read}
        sources:
          demo:
            type: demo_logs
            format: shuffle
            lines: ['${VECTOR_TEST_PARSE_FIRST_UNSET:?must not be read}']
            count: '42'
    "#};
    let builder = ConfigBuilderLoader::default()
        .interpolate_env(false)
        .load_from_input(input.as_bytes(), Format::Yaml)
        .unwrap();
    let value = serde_json::to_value(builder).unwrap();
    assert_eq!(
        value["sources"]["demo"]["lines"],
        json!(["${VECTOR_TEST_PARSE_FIRST_UNSET:?must not be read}"])
    );
    assert_eq!(value["sources"]["demo"]["count"], 42);

    let comment_only = "# ${VECTOR_TEST_PARSE_FIRST_UNSET:?must not be read}\nsources: {}";
    ConfigBuilderLoader::default()
        .interpolate_env(true)
        .load_from_input(comment_only.as_bytes(), Format::Yaml)
        .unwrap();
}

#[test]
fn secrets_cannot_inject_configuration_structure() {
    let secret = "\"\ncount: 999\ninjected: [a, b]\n";
    let input = indoc! {r#"
        sources:
          demo:
            type: demo_logs
            format: shuffle
            lines: ['SECRET[backend.line]']
            count: 'SECRET[backend.count]'
    "#};
    let builder = ConfigBuilderLoader::default()
        .secrets(HashMap::from([
            ("backend.line".into(), secret.into()),
            ("backend.count".into(), "42".into()),
        ]))
        .load_from_input(input.as_bytes(), Format::Yaml)
        .unwrap();
    let value = serde_json::to_value(builder).unwrap();
    assert_eq!(value["sources"]["demo"]["lines"], json!([secret]));
    assert_eq!(value["sources"]["demo"]["count"], 42);
    assert_eq!(value["sources"].as_object().unwrap().len(), 1);
}

#[test]
fn namespaced_files_are_coerced_under_their_component_field() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("sources")).unwrap();
    std::fs::write(
        dir.path().join("sources/demo.yaml"),
        indoc! {r#"
            type: demo_logs
            format: json
            count: ${VECTOR_TEST_PARSE_FIRST_COUNT:-42}
        "#},
    )
    .unwrap();
    let builder = ConfigBuilderLoader::default()
        .interpolate_env(true)
        .load_from_paths(&[ConfigPath::Dir(dir.path().to_owned())])
        .unwrap();
    assert_eq!(
        serde_json::to_value(builder).unwrap()["sources"]["demo"]["count"],
        42
    );
}

#[test]
fn namespaced_tests_coerce_interpolated_event_counts() {
    for (name, count) in [
        (
            "environment variable",
            "${VECTOR_TEST_PARSE_FIRST_COUNT:-42}",
        ),
        ("secret", "SECRET[backend.count]"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("tests")).unwrap();
        std::fs::write(
            dir.path().join("tests/events.yaml"),
            indoc::formatdoc! {"
                name: event count
                outputs:
                  - extract_from: transform
                    expected_event_count: '{count}'
            "},
        )
        .unwrap();

        let builder = ConfigBuilderLoader::default()
            .interpolate_env(true)
            .secrets(HashMap::from([("backend.count".into(), "42".into())]))
            .load_from_paths(&[ConfigPath::Dir(dir.path().to_owned())])
            .unwrap_or_else(|errors| panic!("{name}: {errors:?}"));

        assert_eq!(builder.tests.len(), 1, "{name}");
        assert_eq!(builder.tests[0].name, "event count", "{name}");
        assert_eq!(
            builder.tests[0].outputs[0].expected_event_count,
            Some(42),
            "{name}"
        );
    }
}

#[test]
fn directory_fragments_merge_interpolated_scalars() {
    struct Case {
        name: &'static str,
        path: &'static [&'static str],
        first: Value,
        second: Value,
        expected: Result<Value, &'static str>,
    }

    for case in [
        Case {
            name: "an interpolated boolean overrides a literal",
            path: &["proxy", "enabled"],
            first: json!(false),
            second: json!("${VECTOR_TEST_DIRECTORY_BOOL:-false}"),
            expected: Ok(json!(false)),
        },
        Case {
            name: "a literal overrides an interpolated boolean",
            path: &["proxy", "enabled"],
            first: json!("${VECTOR_TEST_DIRECTORY_BOOL:-true}"),
            second: json!(true),
            expected: Ok(json!(true)),
        },
        Case {
            name: "an interpolated integer in a partial component",
            path: &["sources", "demo", "count"],
            first: json!(43),
            second: json!("${VECTOR_TEST_DIRECTORY_COUNT:-43}"),
            expected: Ok(json!(43)),
        },
        Case {
            name: "a literal overrides an interpolated integer",
            path: &["sources", "demo", "count"],
            first: json!("${VECTOR_TEST_DIRECTORY_COUNT:-43}"),
            second: json!(43),
            expected: Ok(json!(43)),
        },
        Case {
            name: "an interpolated float in a partial component",
            path: &["sources", "demo", "interval"],
            first: json!(2.5),
            second: json!("${VECTOR_TEST_DIRECTORY_INTERVAL:-2.5}"),
            expected: Ok(json!(2.5)),
        },
        Case {
            name: "a secret overrides a literal integer",
            path: &["sources", "demo", "count"],
            first: json!(43),
            second: json!("SECRET[backend.count]"),
            expected: Ok(json!(43)),
        },
        Case {
            name: "a literal overrides a secret",
            path: &["sources", "demo", "count"],
            first: json!("SECRET[backend.count]"),
            second: json!(43),
            expected: Ok(json!(43)),
        },
        Case {
            name: "an invalid interpolated value cannot be hidden by an override",
            path: &["proxy", "enabled"],
            first: json!("${VECTOR_TEST_DIRECTORY_INVALID:-invalid}"),
            second: json!(true),
            expected: Err("proxy.enabled"),
        },
        Case {
            name: "a boolean cannot override an integer",
            path: &["proxy", "enabled"],
            first: json!(1),
            second: json!(true),
            expected: Err("Incompatible types"),
        },
        Case {
            name: "an integer cannot override a float",
            path: &["sources", "demo", "interval"],
            first: json!(1.5),
            second: json!(2),
            expected: Err("Incompatible types"),
        },
        Case {
            name: "an array cannot be replaced with a string",
            path: &["sources", "demo", "lines"],
            first: json!(["first"]),
            second: json!("second"),
            expected: Err("Incompatible types"),
        },
    ] {
        let dir = tempfile::tempdir().unwrap();
        // Component type and required fields are deliberately in separate files.
        // Directory traversal order is unspecified, so successful cases use equal values.
        let fragments = [
            json!({"sources": {"demo": {"type": "demo_logs"}}}),
            json!({"sources": {"demo": {"format": "json"}}}),
            directory_fragment(case.path, case.first),
            directory_fragment(case.path, case.second),
        ];
        for (index, fragment) in fragments.iter().enumerate() {
            std::fs::write(
                dir.path().join(format!("{index}.yaml")),
                serde_yaml::to_string(fragment).unwrap(),
            )
            .unwrap();
        }

        let result = ConfigBuilderLoader::default()
            .interpolate_env(true)
            .secrets(HashMap::from([("backend.count".into(), "43".into())]))
            .load_from_paths(&[ConfigPath::Dir(dir.path().to_owned())]);
        match case.expected {
            Ok(expected) => {
                let builder = result.unwrap_or_else(|errors| panic!("{}: {errors:?}", case.name));
                let actual = if case.path == ["proxy", "enabled"] {
                    // The default value is deliberately omitted by serialization.
                    json!(builder.global.proxy.enabled)
                } else {
                    let value = serde_json::to_value(builder).unwrap();
                    let pointer = format!("/{}", case.path.join("/"));
                    value.pointer(&pointer).unwrap().clone()
                };
                assert_eq!(actual, expected, "{}", case.name);
            }
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

fn directory_fragment(path: &[&str], value: Value) -> Value {
    path.iter()
        .rev()
        .fold(value, |value, key| json!({*key: value}))
}

#[tokio::test]
async fn directory_secret_discovery_defers_unresolved_component_values() {
    let dir = tempfile::tempdir().unwrap();
    let secrets_dir = tempfile::tempdir().unwrap();
    let secrets_path = secrets_dir.path().join("secrets.json");
    std::fs::write(&secrets_path, r#"{"count":"43"}"#).unwrap();
    let fragments = [
        json!({
            "secret": {"backend": {"type": "file"}},
            "sources": {"demo": {"type": "demo_logs", "format": "json", "count": 43}},
            "proxy": {"enabled": true}
        }),
        json!({
            "secret": {"backend": {"path": secrets_path}},
            "sources": {"demo": {"count": "SECRET[backend.count]"}},
            "proxy": {"enabled": "${VECTOR_TEST_DIRECTORY_BOOL:-true}"}
        }),
    ];
    for (index, fragment) in fragments.iter().enumerate() {
        std::fs::write(
            dir.path().join(format!("{index}.yaml")),
            serde_yaml::to_string(fragment).unwrap(),
        )
        .unwrap();
    }

    let paths = [ConfigPath::Dir(dir.path().to_owned())];
    let loader =
        loader_from_paths(SecretBackendLoader::default().interpolate_env(true), &paths).unwrap();
    let (mut signal_handler, _receiver) = crate::signal::SignalHandler::new();
    let secrets = loader.retrieve_secrets(&mut signal_handler).await.unwrap();
    assert_eq!(
        secrets,
        HashMap::from([("backend.count".into(), "43".into())])
    );

    let builder = ConfigBuilderLoader::default()
        .interpolate_env(true)
        .secrets(secrets)
        .load_from_paths(&paths)
        .unwrap();
    assert!(builder.global.proxy.enabled);
    let value = serde_json::to_value(builder).unwrap();
    assert_eq!(value["sources"]["demo"]["count"], 43);
}

#[test]
fn coercion_errors_include_the_component_field_path() {
    let input = indoc! {r#"
        sources:
          demo:
            type: demo_logs
            format: json
            count: ${VECTOR_TEST_PARSE_FIRST_BAD_COUNT:-invalid}
    "#};
    let errors = ConfigBuilderLoader::default()
        .interpolate_env(true)
        .load_from_input(input.as_bytes(), Format::Yaml)
        .unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.contains("sources.demo.count")),
        "{errors:?}"
    );
}

#[test]
fn invalid_unquoted_placeholders_include_migration_guidance() {
    for (format, input) in [
        (
            Format::Toml,
            indoc! {r#"
                [sources.demo]
                type = 'demo_logs'
                format = 'json'
                count = ${VECTOR_TEST_PARSE_FIRST_COUNT:-42}
            "#},
        ),
        (
            Format::Json,
            r#"{"sources":{"demo":{"type":"demo_logs","format":"json","count":SECRET[backend.count]}}}"#,
        ),
    ] {
        let errors = ConfigBuilderLoader::default()
            .interpolate_env(true)
            .load_from_input(input.as_bytes(), format)
            .unwrap_err();
        assert!(
            errors
                .iter()
                .any(|error| error.contains("Quote placeholders")),
            "{errors:?}"
        );
    }
}

#[test]
fn source_loader_preserves_placeholders_and_types() {
    let input = "# ${VECTOR_TEST_PARSE_FIRST_UNSET:?ignored}\n${KEY}: '${VALUE}'\ncount: '42'\nsecret: 'SECRET[backend.key]'\n";
    let map = loader_from_input(SourceLoader::new(), input.as_bytes(), Format::Yaml).unwrap();
    assert_eq!(
        Value::Object(map),
        json!({"${KEY}": "${VALUE}", "count": "42", "secret": "SECRET[backend.key]"})
    );
}

#[tokio::test]
async fn backend_loading_defers_component_coercion_until_secrets_are_resolved() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("secrets.json");
    std::fs::write(&path, r#"{"type":"demo_logs","count":"42"}"#).unwrap();
    let input = serde_yaml::to_string(&json!({
        "secret": {"local": {"type": "file", "path": path}},
        "sources": {"demo": {"type": "SECRET[local.type]", "count": "SECRET[local.count]"}},
        "SECRET[missing.key]": "not a secret reference"
    }))
    .unwrap();
    let input = format!("# SECRET[missing.comment]\n{input}");
    let loader: SecretBackendLoader = loader_from_input(
        SecretBackendLoader::default(),
        input.as_bytes(),
        Format::Yaml,
    )
    .unwrap();
    let (mut signal_handler, _receiver) = crate::signal::SignalHandler::new();
    let secrets = loader.retrieve_secrets(&mut signal_handler).await.unwrap();
    assert_eq!(
        secrets,
        HashMap::from([
            ("local.type".into(), "demo_logs".into()),
            ("local.count".into(), "42".into())
        ])
    );

    let input = indoc! {r#"
        sources:
          demo:
            type: SECRET[local.type]
            format: json
            count: SECRET[local.count]
    "#};
    let builder = ConfigBuilderLoader::default()
        .secrets(secrets)
        .load_from_input(input.as_bytes(), Format::Yaml)
        .unwrap();
    assert_eq!(
        serde_json::to_value(builder).unwrap()["sources"]["demo"]["count"],
        42
    );
}
