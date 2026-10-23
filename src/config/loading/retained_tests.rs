use std::{collections::HashMap, path::Path};

use indoc::indoc;
use serde_json::{Value, json};

use super::{
    ConfigBuilderLoader, SecretBackendLoader, load_builder_from_prepared_with_secrets,
    loader::ParsedInputs,
};
use crate::config::{ConfigPath, Format};

fn write_yaml(path: &Path, value: &Value) {
    std::fs::write(path, serde_yaml::to_string(value).unwrap()).unwrap();
}

#[tokio::test]
async fn secret_discovery_and_loading_use_the_same_file_and_directory_snapshot() {
    const ORIGINAL_CONFIG: &str = indoc! {r#"
        secret:
          local:
            type: file
            path: ""
        sources:
          demo:
            type: demo_logs
            format: json
            count: SECRET[local.original]
    "#};
    const ADDED_CONFIG: &str = indoc! {r"
        sources:
          added:
            type: demo_logs
            format: json
    "};

    for directory in [false, true] {
        let name = if directory { "directory" } else { "file" };
        let temp = tempfile::tempdir().unwrap();
        let config_dir = temp.path().join("config");
        std::fs::create_dir(&config_dir).unwrap();
        let config_path = config_dir.join("vector.yaml");
        let secrets_path = temp.path().join("secrets.json");
        std::fs::write(&secrets_path, r#"{"original":"41","replacement":"42"}"#).unwrap();
        let mut original: Value = serde_yaml::from_str(ORIGINAL_CONFIG).unwrap();
        original["secret"]["local"]["path"] = json!(secrets_path);
        write_yaml(&config_path, &original);
        let paths = [if directory {
            ConfigPath::Dir(config_dir.clone())
        } else {
            ConfigPath::File(config_path.clone(), Some(Format::Yaml))
        }];

        let mut retained = ParsedInputs::from_paths(&paths);
        retained.interpolate_environment(false);
        let backends = SecretBackendLoader::default()
            .load_prepared(&retained)
            .unwrap_or_else(|errors| panic!("{name}: {errors:?}"));

        let mut replacement = original;
        replacement["sources"]["demo"]["count"] = json!("SECRET[local.replacement]");
        write_yaml(&config_path, &replacement);
        if directory {
            std::fs::write(config_dir.join("added.yaml"), ADDED_CONFIG).unwrap();
        }

        let (mut signals, _receiver) = crate::signal::SignalHandler::new();
        let secrets = backends.retrieve_secrets(&mut signals).await.unwrap();
        assert_eq!(
            secrets,
            HashMap::from([("local.original".into(), "41".into())]),
            "{name}"
        );
        retained.substitute_secrets(&secrets);
        let builder = ConfigBuilderLoader::default()
            .load_prepared(&retained)
            .unwrap_or_else(|errors| panic!("{name}: {errors:?}"));
        let value = serde_json::to_value(builder).unwrap();
        assert_eq!(value["sources"]["demo"]["count"], 41, "{name}");
        assert!(value["sources"].get("added").is_none(), "{name}");

        let mut refreshed = ParsedInputs::from_paths(&paths);
        refreshed.interpolate_environment(false);
        let builder = load_builder_from_prepared_with_secrets(refreshed, &mut signals, false)
            .await
            .unwrap_or_else(|errors| panic!("fresh {name}: {errors:?}"));
        let value = serde_json::to_value(builder).unwrap();
        assert_eq!(value["sources"]["demo"]["count"], 42, "fresh {name}");
        assert_eq!(value["sources"].get("added").is_some(), directory);
    }
}

#[tokio::test]
async fn prepared_inputs_apply_environment_then_secrets_once() {
    struct Case {
        name: &'static str,
        input: &'static str,
        secret: &'static str,
        expected: &'static str,
    }

    for case in [
        Case {
            name: "an escaped environment reference is not expanded a second time",
            input: "$${VECTOR_TEST_RETAINED_UNSET:?expanded twice}",
            secret: "unused",
            expected: "${VECTOR_TEST_RETAINED_UNSET:?expanded twice}",
        },
        Case {
            name: "environment expansion can introduce a secret reference",
            input: "${VECTOR_TEST_RETAINED_UNSET:-SECRET[local.line]}",
            secret: "resolved secret",
            expected: "resolved secret",
        },
        Case {
            name: "secret values are not interpolated recursively",
            input: "SECRET[local.line]",
            secret: "$VECTOR_TEST_RETAINED_UNSET SECRET[missing.recursive]",
            expected: "$VECTOR_TEST_RETAINED_UNSET SECRET[missing.recursive]",
        },
    ] {
        let temp = tempfile::tempdir().unwrap();
        let secrets_path = temp.path().join("secrets.json");
        std::fs::write(
            &secrets_path,
            serde_json::to_vec(&json!({"line": case.secret})).unwrap(),
        )
        .unwrap();
        let input = serde_yaml::to_string(&json!({
            "secret": {"local": {"type": "file", "path": secrets_path}},
            "sources": {"demo": {"type": "demo_logs", "format": "shuffle", "lines": [case.input]}}
        }))
        .unwrap();
        let mut prepared = ParsedInputs::from_input(input.as_bytes(), Format::Yaml);
        prepared.interpolate_environment(true);
        let (mut signals, _receiver) = crate::signal::SignalHandler::new();
        let builder = load_builder_from_prepared_with_secrets(prepared, &mut signals, false)
            .await
            .unwrap_or_else(|errors| panic!("{}: {errors:?}", case.name));

        assert_eq!(
            serde_json::to_value(builder).unwrap()["sources"]["demo"]["lines"],
            json!([case.expected]),
            "{}",
            case.name
        );
    }
}

#[tokio::test]
async fn secret_discovery_includes_overwritten_directory_references() {
    const FIRST_CONFIG: &str = indoc! {r#"
        secret:
          local:
            type: file
            path: ""
        sources:
          demo:
            type: demo_logs
            format: json
            count: SECRET[local.first]
    "#};
    const SECOND_CONFIG: &str = indoc! {r"
        sources:
          demo:
            type: demo_logs
            format: json
            count: SECRET[local.second]
    "};

    let temp = tempfile::tempdir().unwrap();
    let config_dir = temp.path().join("config");
    std::fs::create_dir(&config_dir).unwrap();
    let secrets_path = temp.path().join("secrets.json");
    std::fs::write(&secrets_path, r#"{"first":"43","second":"43"}"#).unwrap();
    for (key, input) in [("first", FIRST_CONFIG), ("second", SECOND_CONFIG)] {
        let mut config: Value = serde_yaml::from_str(input).unwrap();
        if key == "first" {
            config["secret"]["local"]["path"] = json!(secrets_path);
        }
        write_yaml(&config_dir.join(format!("{key}.yaml")), &config);
    }

    let mut prepared = ParsedInputs::from_paths(&[ConfigPath::Dir(config_dir)]);
    prepared.interpolate_environment(false);
    let backends = SecretBackendLoader::default()
        .load_prepared(&prepared)
        .unwrap();
    let (mut signals, _receiver) = crate::signal::SignalHandler::new();
    let secrets = backends.retrieve_secrets(&mut signals).await.unwrap();
    assert_eq!(
        secrets,
        HashMap::from([
            ("local.first".into(), "43".into()),
            ("local.second".into(), "43".into()),
        ])
    );
    prepared.substitute_secrets(&secrets);
    let builder = ConfigBuilderLoader::default()
        .load_prepared(&prepared)
        .unwrap();
    assert_eq!(
        serde_json::to_value(builder).unwrap()["sources"]["demo"]["count"],
        43
    );
}

#[tokio::test]
async fn inline_values_exclude_shadowed_nested_directories_from_loading() {
    const PARENT: &str = indoc! {r"
        inline:
          retained: literal
    "};
    const INVALID_CONFIG: &str = "[invalid YAML";
    const REQUIRED_ENVIRONMENT: &str =
        "value: '${VECTOR_TEST_RETAINED_UNSET:?shadowed environment}'";
    const SECRET_REFERENCE: &str = "value: SECRET[missing.shadowed]";

    struct Case {
        name: &'static str,
        shadowed_input: &'static str,
    }

    for case in [
        Case {
            name: "shadowed parse errors are ignored",
            shadowed_input: INVALID_CONFIG,
        },
        Case {
            name: "shadowed environment errors are ignored",
            shadowed_input: REQUIRED_ENVIRONMENT,
        },
        Case {
            name: "shadowed files do not contribute secret references",
            shadowed_input: SECRET_REFERENCE,
        },
    ] {
        let temp = tempfile::tempdir().unwrap();
        let transforms = temp.path().join("transforms");
        let shadowed = transforms.join("outer/inline");
        std::fs::create_dir_all(&shadowed).unwrap();
        std::fs::write(transforms.join("outer.yaml"), PARENT).unwrap();
        std::fs::write(shadowed.join("ignored.yaml"), case.shadowed_input).unwrap();

        let mut prepared = ParsedInputs::from_paths(&[ConfigPath::Dir(temp.path().to_owned())]);
        prepared.interpolate_environment(true);
        let backends = SecretBackendLoader::default()
            .load_prepared(&prepared)
            .unwrap_or_else(|errors| panic!("{}: {errors:?}", case.name));
        let (mut signals, _receiver) = crate::signal::SignalHandler::new();
        let secrets = backends
            .retrieve_secrets(&mut signals)
            .await
            .unwrap_or_else(|error| panic!("{}: {error}", case.name));
        assert!(secrets.is_empty(), "{}", case.name);
    }
}

#[test]
fn retained_inputs_collect_parse_interpolation_and_deserialization_errors() {
    const INVALID_JSON: &str = "{";
    const REQUIRED_ENVIRONMENT: &str = indoc! {r"
        data_dir: ${VECTOR_TEST_RETAINED_UNSET:?retained environment failure}
    "};
    const INVALID_COUNT: &str = indoc! {r"
        sources:
          demo:
            type: demo_logs
            format: json
            count: definitely_not_a_count
    "};

    let temp = tempfile::tempdir().unwrap();
    let invalid_json = temp.path().join("invalid.json");
    std::fs::write(&invalid_json, INVALID_JSON).unwrap();
    let environment = temp.path().join("environment.yaml");
    std::fs::write(&environment, REQUIRED_ENVIRONMENT).unwrap();
    let directory = temp.path().join("config");
    std::fs::create_dir(&directory).unwrap();
    std::fs::write(directory.join("invalid-count.yaml"), INVALID_COUNT).unwrap();

    let mut prepared = ParsedInputs::from_paths(&[
        ConfigPath::File(invalid_json, Some(Format::Json)),
        ConfigPath::File(environment, Some(Format::Yaml)),
        ConfigPath::Dir(directory),
    ]);
    prepared.interpolate_environment(true);
    let errors = ConfigBuilderLoader::default()
        .load_prepared(&prepared)
        .unwrap_err();
    for expected in [
        "EOF while parsing an object",
        "retained environment failure",
        "sources.demo.count",
    ] {
        assert!(
            errors.iter().any(|error| error.contains(expected)),
            "missing {expected:?}: {errors:?}"
        );
    }
}
