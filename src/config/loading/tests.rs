use std::collections::HashMap;

use indoc::indoc;
use serde_json::{Value, json};

use super::{ConfigBuilderLoader, SecretBackendLoader, SourceLoader, loader_from_input};
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
