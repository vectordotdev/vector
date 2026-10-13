use std::{collections::HashMap, path::Path};

#[cfg(feature = "sources-demo_logs")]
use indoc::indoc;
use serde_json::{Value, json};

use super::with_absent_env_var;
#[cfg(any(feature = "sources-demo_logs", feature = "sinks-console"))]
use crate::config::Format;
use crate::config::{
    ConfigPath,
    loading::{ConfigBuilderLoader, load_source_from_paths},
};

fn write_files(root: &Path, files: &[(&str, &str)]) {
    for (name, content) in files {
        let path = root.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, with_absent_env_var(content)).unwrap();
    }
}

#[test]
#[cfg(feature = "sinks-console")]
fn healthchecks_accept_boolean_shorthand_and_objects() {
    struct Case {
        name: &'static str,
        input: Value,
        enabled: Option<bool>,
    }

    for case in [
        Case {
            name: "false",
            input: json!(false),
            enabled: Some(false),
        },
        Case {
            name: "true",
            input: json!(true),
            enabled: Some(true),
        },
        Case {
            name: "object false",
            input: json!({"enabled": false}),
            enabled: Some(false),
        },
        Case {
            name: "default object",
            input: json!({}),
            enabled: Some(true),
        },
        Case {
            name: "quoted environment boolean is a string",
            input: json!("${VECTOR_TEST_COMPAT_UNSET:-false}"),
            enabled: None,
        },
        Case {
            name: "quoted secret boolean is a string",
            input: json!("SECRET[backend.healthcheck]"),
            enabled: None,
        },
        Case {
            name: "integer",
            input: json!(1),
            enabled: None,
        },
        Case {
            name: "null",
            input: Value::Null,
            enabled: None,
        },
        Case {
            name: "invalid string",
            input: json!("disabled"),
            enabled: None,
        },
        Case {
            name: "array",
            input: json!([false]),
            enabled: None,
        },
    ] {
        let input = json!({"sinks": {"console": {
            "type": "console", "inputs": [], "encoding": {"codec": "json"}, "healthcheck": case.input
        }}}).to_string();
        let input = with_absent_env_var(&input);
        let result = ConfigBuilderLoader::default()
            .interpolate_env(true)
            .secrets(HashMap::from([(
                "backend.healthcheck".into(),
                "false".into(),
            )]))
            .load_from_input(input.as_bytes(), Format::Json);
        if let Some(enabled) = case.enabled {
            let builder = result.unwrap_or_else(|errors| panic!("{}: {errors:?}", case.name));
            assert_eq!(
                serde_json::to_value(builder).unwrap()["sinks"]["console"]["healthcheck"]["enabled"],
                enabled,
                "{}",
                case.name
            );
        } else {
            assert!(result.is_err(), "{}", case.name);
        }
    }
}

#[test]
#[cfg(feature = "sources-demo_logs")]
fn namespaced_files_load_interpolated_scalars() {
    let directory = tempfile::tempdir().unwrap();
    write_files(
        directory.path(),
        &[(
            "sources/demo.yaml",
            indoc! {"
        type: demo_logs
        format: json
        count: ${VECTOR_TEST_COMPAT_UNSET:-42}
        batch_interval: ${VECTOR_TEST_COMPAT_UNSET:-1.5}
        log_namespace: ${VECTOR_TEST_COMPAT_UNSET:-true}
    "},
        )],
    );
    let builder = ConfigBuilderLoader::default()
        .interpolate_env(true)
        .load_from_paths(&[ConfigPath::Dir(directory.path().to_owned())])
        .unwrap();
    let value = serde_json::to_value(builder).unwrap();
    let source = &value["sources"]["demo"];
    assert_eq!(source["count"], 42);
    assert_eq!(source["interval"], 1.5);
    assert_eq!(source["log_namespace"], true);
}

#[test]
fn namespaced_tests_load_interpolated_event_counts() {
    for (name, count) in [
        ("environment", "${VECTOR_TEST_COMPAT_UNSET:-42}"),
        ("secret", "SECRET[backend.count]"),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let input = format!(
            "name: event count\noutputs:\n  - extract_from: transform\n    expected_event_count: {count}\n"
        );
        write_files(directory.path(), &[("tests/events.yaml", &input)]);
        let builder = ConfigBuilderLoader::default()
            .interpolate_env(true)
            .secrets(HashMap::from([("backend.count".into(), "42".into())]))
            .load_from_paths(&[ConfigPath::Dir(directory.path().to_owned())])
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
#[cfg(feature = "sources-demo_logs")]
#[allow(
    clippy::too_many_lines,
    reason = "Keep the merge cases with their shared setup and assertions"
)]
fn directory_fragments_preserve_merge_semantics() {
    struct Case {
        name: &'static str,
        field: &'static str,
        first: &'static str,
        second: &'static str,
        expected: Option<Value>,
    }

    for case in [
        Case {
            name: "equal integers merge",
            field: "count",
            first: "43",
            second: "43",
            expected: Some(json!(43)),
        },
        Case {
            name: "environment integer merges with literal",
            field: "count",
            first: "43",
            second: "${VECTOR_TEST_COMPAT_UNSET:-43}",
            expected: Some(json!(43)),
        },
        Case {
            name: "secret integer merges with literal",
            field: "count",
            first: "SECRET[backend.count]",
            second: "43",
            expected: Some(json!(43)),
        },
        Case {
            name: "environment float merges with literal",
            field: "interval",
            first: "2.5",
            second: "${VECTOR_TEST_COMPAT_UNSET:-2.5}",
            expected: Some(json!(2.5)),
        },
        Case {
            name: "environment boolean merges with literal",
            field: "log_namespace",
            first: "true",
            second: "${VECTOR_TEST_COMPAT_UNSET:-true}",
            expected: Some(json!(true)),
        },
        Case {
            name: "integer conflicts with boolean",
            field: "count",
            first: "1",
            second: "true",
            expected: None,
        },
        Case {
            name: "integer conflicts with float",
            field: "interval",
            first: "1.5",
            second: "2",
            expected: None,
        },
        Case {
            name: "array conflicts with string",
            field: "lines",
            first: "[first]",
            second: "second",
            expected: None,
        },
        Case {
            name: "invalid interpolation cannot be hidden",
            field: "count",
            first: "${VECTOR_TEST_COMPAT_UNSET:-invalid}",
            second: "43",
            expected: None,
        },
        Case {
            name: "quoted interpolation conflicts with integer",
            field: "count",
            first: "'${VECTOR_TEST_COMPAT_UNSET:-43}'",
            second: "43",
            expected: None,
        },
    ] {
        for reversed in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let fragments = if reversed {
                [case.second, case.first]
            } else {
                [case.first, case.second]
            };
            write_files(
                directory.path(),
                &[
                    ("type.yaml", "sources:\n  demo:\n    type: demo_logs\n"),
                    ("format.yaml", "sources:\n  demo:\n    format: json\n"),
                ],
            );
            for (index, value) in fragments.iter().enumerate() {
                let input = format!("sources:\n  demo:\n    {}: {value}\n", case.field);
                write_files(
                    directory.path(),
                    &[(&format!("fragment-{index}.yaml"), &input)],
                );
            }
            // Directory traversal is unordered. Successful cases use equal values;
            // conflicting types must be rejected regardless of which file is visited first.
            let result = ConfigBuilderLoader::default()
                .interpolate_env(true)
                .secrets(HashMap::from([("backend.count".into(), "43".into())]))
                .load_from_paths(&[ConfigPath::Dir(directory.path().to_owned())]);
            if let Some(expected) = &case.expected {
                let builder = result.unwrap_or_else(|errors| {
                    panic!("{} / reversed={reversed}: {errors:?}", case.name)
                });
                assert_eq!(
                    &serde_json::to_value(builder).unwrap()["sources"]["demo"][case.field],
                    expected,
                    "{} / reversed={reversed}",
                    case.name
                );
            } else {
                let errors = result.expect_err(case.name);
                assert!(
                    errors
                        .iter()
                        .any(|error| error.contains("Incompatible types")),
                    "{} / reversed={reversed}: {errors:?}",
                    case.name
                );
            }
        }
    }
}

#[test]
#[cfg(feature = "sources-demo_logs")]
fn file_and_directory_inputs_preserve_append_boundaries() {
    struct Case {
        name: &'static str,
        paths: fn(&Path, &Path) -> Vec<ConfigPath>,
        duplicate: bool,
    }

    for case in [
        Case {
            name: "files in one directory merge",
            paths: |first, _| vec![ConfigPath::Dir(first.parent().unwrap().to_owned())],
            duplicate: false,
        },
        Case {
            name: "separate files remain separate builders",
            paths: |first, second| {
                vec![
                    ConfigPath::File(first.to_owned(), Some(Format::Yaml)),
                    ConfigPath::File(second.to_owned(), Some(Format::Yaml)),
                ]
            },
            duplicate: true,
        },
        Case {
            name: "file followed by directory",
            paths: |first, _| {
                vec![
                    ConfigPath::File(first.to_owned(), Some(Format::Yaml)),
                    ConfigPath::Dir(first.parent().unwrap().to_owned()),
                ]
            },
            duplicate: true,
        },
        Case {
            name: "directory followed by file",
            paths: |first, _| {
                vec![
                    ConfigPath::Dir(first.parent().unwrap().to_owned()),
                    ConfigPath::File(first.to_owned(), Some(Format::Yaml)),
                ]
            },
            duplicate: true,
        },
    ] {
        let directory = tempfile::tempdir().unwrap();
        let input = "sources:\n  demo:\n    type: demo_logs\n    format: json\n    count: 43\n";
        write_files(
            directory.path(),
            &[("first.yaml", input), ("second.yaml", input)],
        );
        let result = ConfigBuilderLoader::default()
            .interpolate_env(false)
            .load_from_paths(&(case.paths)(
                &directory.path().join("first.yaml"),
                &directory.path().join("second.yaml"),
            ));
        if case.duplicate {
            let errors = result.expect_err(case.name);
            assert!(
                errors
                    .iter()
                    .any(|error| error.contains("duplicate source id found: demo")),
                "{}: {errors:?}",
                case.name
            );
        } else {
            let builder = result.unwrap_or_else(|errors| panic!("{}: {errors:?}", case.name));
            assert_eq!(
                serde_json::to_value(builder).unwrap()["sources"]["demo"]["count"],
                43,
                "{}",
                case.name
            );
        }
    }
}

#[test]
fn inline_fields_shadow_matching_directories_but_not_files() {
    struct Case {
        name: &'static str,
        nested_path: &'static str,
        content: &'static str,
        expected: Value,
    }

    for case in [
        Case {
            name: "invalid YAML in shadowed folder",
            nested_path: "inline/ignored.yaml",
            content: "invalid: [yaml",
            expected: json!({"retained": true}),
        },
        Case {
            name: "required environment in shadowed folder",
            nested_path: "inline/ignored.yaml",
            content: "value: ${VECTOR_TEST_COMPAT_UNSET:?must not be read}",
            expected: json!({"retained": true}),
        },
        Case {
            name: "secret in shadowed folder",
            nested_path: "inline/ignored.yaml",
            content: "value: SECRET[missing.shadowed]",
            expected: json!({"retained": true}),
        },
        Case {
            name: "nested file still contributes",
            nested_path: "inline.yaml",
            content: "from_file: true",
            expected: json!({"retained": true, "from_file": true}),
        },
    ] {
        let directory = tempfile::tempdir().unwrap();
        write_files(
            directory.path(),
            &[
                ("transforms/parent.yaml", "inline:\n  retained: true\n"),
                (
                    &format!("transforms/parent/{}", case.nested_path),
                    case.content,
                ),
            ],
        );
        let map = load_source_from_paths(&[ConfigPath::Dir(directory.path().to_owned())])
            .unwrap_or_else(|errors| panic!("{}: {errors:?}", case.name));
        assert_eq!(map["parent"]["inline"], case.expected, "{}", case.name);
    }
}

#[test]
fn same_stem_files_preserve_nested_contributions() {
    let directory = tempfile::tempdir().unwrap();
    write_files(
        directory.path(),
        &[
            ("transforms/parent.yaml", "child:\n  inline: true\n"),
            ("transforms/parent.json", r#"{"other":true}"#),
            ("transforms/parent/child/disk.yaml", "nested: true\n"),
        ],
    );
    let map = load_source_from_paths(&[ConfigPath::Dir(directory.path().to_owned())]).unwrap();
    assert_eq!(
        Value::Object(map),
        json!({"parent": {"child": {"inline": true, "disk": {"nested": true}}, "other": true}})
    );
}

#[test]
#[cfg(feature = "sources-demo_logs")]
fn interpolated_scalars_follow_format_quoting() {
    struct Case {
        name: &'static str,
        format: Format,
        template: &'static str,
        accepted: bool,
    }

    // These are the text-first loader's expectations. Quoted numeric/boolean
    // placeholders are strings, not values of the destination field's type.
    for case in [
        Case {
            name: "YAML plain placeholders",
            format: Format::Yaml,
            template: "sources:\n  demo:\n    type: demo_logs\n    format: json\n    count: @COUNT@\n    interval: @INTERVAL@\n    log_namespace: @BOOL@\n",
            accepted: true,
        },
        Case {
            name: "YAML quoted placeholders",
            format: Format::Yaml,
            template: "sources:\n  demo:\n    type: demo_logs\n    format: json\n    count: '@COUNT@'\n    interval: '@INTERVAL@'\n    log_namespace: '@BOOL@'\n",
            accepted: false,
        },
        Case {
            name: "TOML unquoted placeholders",
            format: Format::Toml,
            template: "[sources.demo]\ntype = 'demo_logs'\nformat = 'json'\ncount = @COUNT@\ninterval = @INTERVAL@\nlog_namespace = @BOOL@\n",
            accepted: true,
        },
        Case {
            name: "TOML quoted placeholders",
            format: Format::Toml,
            template: "[sources.demo]\ntype = 'demo_logs'\nformat = 'json'\ncount = '@COUNT@'\ninterval = '@INTERVAL@'\nlog_namespace = '@BOOL@'\n",
            accepted: false,
        },
        Case {
            name: "JSON unquoted placeholders",
            format: Format::Json,
            template: r#"{"sources":{"demo":{"type":"demo_logs","format":"json","count":@COUNT@,"interval":@INTERVAL@,"log_namespace":@BOOL@}}}"#,
            accepted: true,
        },
        Case {
            name: "JSON quoted placeholders",
            format: Format::Json,
            template: r#"{"sources":{"demo":{"type":"demo_logs","format":"json","count":"@COUNT@","interval":"@INTERVAL@","log_namespace":"@BOOL@"}}}"#,
            accepted: false,
        },
    ] {
        for (source, count, interval, boolean) in [
            (
                "environment",
                "${VECTOR_TEST_COMPAT_UNSET:-42}",
                "${VECTOR_TEST_COMPAT_UNSET:-1.5}",
                "${VECTOR_TEST_COMPAT_UNSET:-true}",
            ),
            (
                "secret",
                "SECRET[backend.count]",
                "SECRET[backend.interval]",
                "SECRET[backend.bool]",
            ),
        ] {
            let input = case
                .template
                .replace("@COUNT@", count)
                .replace("@INTERVAL@", interval)
                .replace("@BOOL@", boolean);
            let input = with_absent_env_var(&input);
            let result = ConfigBuilderLoader::default()
                .interpolate_env(true)
                .secrets(HashMap::from([
                    ("backend.count".into(), "42".into()),
                    ("backend.interval".into(), "1.5".into()),
                    ("backend.bool".into(), "true".into()),
                ]))
                .load_from_input(input.as_bytes(), case.format);
            if case.accepted {
                let builder =
                    result.unwrap_or_else(|errors| panic!("{} / {source}: {errors:?}", case.name));
                let value = serde_json::to_value(builder).unwrap();
                let source_config = &value["sources"]["demo"];
                assert_eq!(source_config["count"], 42, "{} / {source}", case.name);
                assert_eq!(source_config["interval"], 1.5, "{} / {source}", case.name);
                assert_eq!(
                    source_config["log_namespace"], true,
                    "{} / {source}",
                    case.name
                );
            } else {
                assert!(result.is_err(), "{} / {source}", case.name);
            }
        }
    }
}

#[test]
#[cfg(feature = "sources-demo_logs")]
fn disabled_environment_interpolation_preserves_strings() {
    let input = indoc! {r"
        # ${VECTOR_TEST_COMPAT_DISABLED_UNSET:?must not be read}
        sources:
          demo:
            type: demo_logs
            format: shuffle
            lines: ['${VECTOR_TEST_COMPAT_DISABLED_UNSET:?must not be read}', '$$literal']
            count: 42
    "};
    let builder = ConfigBuilderLoader::default()
        .interpolate_env(false)
        .load_from_input(input.as_bytes(), Format::Yaml)
        .unwrap();
    let value = serde_json::to_value(builder).unwrap();
    assert_eq!(
        value["sources"]["demo"]["lines"],
        json!([
            "${VECTOR_TEST_COMPAT_DISABLED_UNSET:?must not be read}",
            "$$literal"
        ])
    );
    assert_eq!(value["sources"]["demo"]["count"], 42);
}
