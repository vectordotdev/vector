use std::collections::HashMap;

use indoc::indoc;
use serde_json::{Value, json};

use super::super::{ConfigBuilderLoader, SecretBackendLoader, SourceLoader};
use crate::config::{ConfigPath, Format};

#[test]
fn comments_are_not_interpolated() {
    let comment_only = "# ${VECTOR_TEST_PARSE_FIRST_UNSET:?must not be read}\nsources: {}";
    ConfigBuilderLoader::default()
        .interpolate_env(true)
        .load_from_input(comment_only.as_bytes(), Format::Yaml)
        .unwrap();
}

#[test]
fn secrets_cannot_inject_configuration_structure() {
    let secret = "\"\ncount: 999\ninjected: [a, b]\n";
    let input = indoc! {r"
        sources:
          demo:
            type: demo_logs
            format: shuffle
            lines: ['SECRET[backend.line]']
            count: 'SECRET[backend.count]'
    "};
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
fn directory_fragments_merge_global_booleans() {
    const FRAGMENT: &str = indoc! {r"
        proxy:
          enabled: null
    "};

    for (name, first, second, expected) in [
        (
            "environment false",
            json!(false),
            json!("${VECTOR_TEST_COMPAT_UNSET:-false}"),
            Ok(false),
        ),
        (
            "environment true",
            json!("${VECTOR_TEST_COMPAT_UNSET:-true}"),
            json!(true),
            Ok(true),
        ),
        (
            "invalid interpolation",
            json!("${VECTOR_TEST_COMPAT_UNSET:-invalid}"),
            json!(true),
            Err("proxy.enabled"),
        ),
        (
            "incompatible integer",
            json!(1),
            json!(true),
            Err("Incompatible types"),
        ),
    ] {
        for reversed in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let values = if reversed {
                [&second, &first]
            } else {
                [&first, &second]
            };
            for (index, value) in values.into_iter().enumerate() {
                let mut fragment: Value = serde_yaml::from_str(FRAGMENT).unwrap();
                fragment["proxy"]["enabled"] = value.clone();
                let input = serde_yaml::to_string(&fragment).unwrap();
                std::fs::write(
                    dir.path().join(format!("{index}.yaml")),
                    super::with_absent_env_var(&input),
                )
                .unwrap();
            }
            let result = ConfigBuilderLoader::default()
                .interpolate_env(true)
                .load_from_paths(&[ConfigPath::Dir(dir.path().to_owned())]);
            match expected {
                Ok(expected) => assert_eq!(
                    result
                        .unwrap_or_else(|errors| panic!("{name}: {errors:?}"))
                        .global
                        .proxy
                        .enabled,
                    expected,
                    "{name} / reversed={reversed}"
                ),
                Err(expected) => {
                    let errors = result.unwrap_err();
                    assert!(
                        errors.iter().any(|error| error.contains(expected)),
                        "{name} / reversed={reversed}: {errors:?}"
                    );
                }
            }
        }
    }
}

#[tokio::test]
async fn directory_secret_discovery_defers_unresolved_component_values() {
    const FIRST_FRAGMENT: &str = indoc! {r"
        secret:
          backend:
            type: file
        sources:
          demo:
            type: demo_logs
            format: json
            count: 43
        proxy:
          enabled: true
    "};
    const SECOND_FRAGMENT: &str = indoc! {r#"
        secret:
          backend:
            path: ""
        sources:
          demo:
            count: SECRET[backend.count]
        proxy:
          enabled: ${VECTOR_TEST_DIRECTORY_BOOL:-true}
    "#};

    let dir = tempfile::tempdir().unwrap();
    let secrets_dir = tempfile::tempdir().unwrap();
    let secrets_path = secrets_dir.path().join("secrets.json");
    std::fs::write(&secrets_path, r#"{"count":"43"}"#).unwrap();
    let first: Value = serde_yaml::from_str(FIRST_FRAGMENT).unwrap();
    let mut second: Value = serde_yaml::from_str(SECOND_FRAGMENT).unwrap();
    second["secret"]["backend"]["path"] = json!(secrets_path);
    let fragments = [first, second];
    for (index, fragment) in fragments.iter().enumerate() {
        std::fs::write(
            dir.path().join(format!("{index}.yaml")),
            serde_yaml::to_string(fragment).unwrap(),
        )
        .unwrap();
    }

    let paths = [ConfigPath::Dir(dir.path().to_owned())];
    let loader = SecretBackendLoader::default()
        .interpolate_env(true)
        .load_from_paths(&paths)
        .unwrap();
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
    let input = indoc! {r"
        sources:
          demo:
            type: demo_logs
            format: json
            count: ${VECTOR_TEST_PARSE_FIRST_BAD_COUNT:-invalid}
    "};
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
    const TOML: &str = indoc! {r"
        [sources.demo]
        type = 'demo_logs'
        format = 'json'
        count = ${VECTOR_TEST_PARSE_FIRST_COUNT:-42}
    "};
    const JSON: &str = r#"{"sources":{"demo":{"type":"demo_logs","format":"json","count":SECRET[backend.count]}}}"#;
    const YAML_ENV_LIST: &str = "proxy: {no_proxy: [${VECTOR_TEST_COMPAT_UNSET:-localhost}]}";
    const YAML_SECRET_LIST: &str = "proxy: {no_proxy: [SECRET[backend.host]]}";
    const YAML_ENV_MAP: &str = "proxy: {enabled: ${VECTOR_TEST_COMPAT_UNSET:-false}}";
    const YAML_SECRET_MAP: &str = "proxy: {enabled: SECRET[backend.enabled]}";

    for (format, input) in [
        (Format::Toml, TOML),
        (Format::Json, JSON),
        (Format::Yaml, YAML_ENV_LIST),
        (Format::Yaml, YAML_SECRET_LIST),
        (Format::Yaml, YAML_ENV_MAP),
        (Format::Yaml, YAML_SECRET_MAP),
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
fn quoted_yaml_flow_placeholders_are_substituted() {
    const CONFIG: &str = indoc! {r#"
        proxy: {
          enabled: "${VECTOR_TEST_COMPAT_UNSET:-false}",
          http: "SECRET[backend.proxy]",
          no_proxy: ["${VECTOR_TEST_COMPAT_UNSET:-localhost}", "SECRET[backend.host]"]
        }
    "#};
    let input = super::with_absent_env_var(CONFIG);
    let builder = ConfigBuilderLoader::default()
        .interpolate_env(true)
        .secrets(HashMap::from([
            ("backend.proxy".into(), "http://proxy.example:8080".into()),
            ("backend.host".into(), "internal.example".into()),
        ]))
        .load_from_input(input.as_bytes(), Format::Yaml)
        .unwrap();
    let value = serde_json::to_value(builder).unwrap();
    assert_eq!(value["proxy"]["enabled"], false);
    assert_eq!(value["proxy"]["http"], "http://proxy.example:8080");
    let mut no_proxy: Vec<_> = value["proxy"]["no_proxy"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect();
    no_proxy.sort_unstable();
    assert_eq!(no_proxy, ["internal.example", "localhost"]);
}

#[test]
fn source_loader_preserves_placeholders_and_types() {
    let input = "# ${VECTOR_TEST_PARSE_FIRST_UNSET:?ignored}\n${KEY}: '${VALUE}'\ncount: '42'\nsecret: 'SECRET[backend.key]'\n";
    let map = SourceLoader::new()
        .load_from_input(input.as_bytes(), Format::Yaml)
        .unwrap();
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
    let loader = SecretBackendLoader::default()
        .load_from_input(input.as_bytes(), Format::Yaml)
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

    let input = indoc! {r"
        sources:
          demo:
            type: SECRET[local.type]
            format: json
            count: SECRET[local.count]
    "};
    let builder = ConfigBuilderLoader::default()
        .secrets(secrets)
        .load_from_input(input.as_bytes(), Format::Yaml)
        .unwrap();
    assert_eq!(
        serde_json::to_value(builder).unwrap()["sources"]["demo"]["count"],
        42
    );
}
