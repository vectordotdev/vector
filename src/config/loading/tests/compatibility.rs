use std::{collections::HashMap, path::Path};

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
fn no_proxy_preserves_string_and_list_matching() {
    for (input, matches_hosts) in [
        (json!("localhost,127.0.0.1"), true),
        (json!(["localhost", "127.0.0.1"]), true),
        (json!(["localhost,127.0.0.1"]), false),
        (
            json!("${VECTOR_TEST_COMPAT_UNSET:-localhost,127.0.0.1}"),
            true,
        ),
        (
            json!(["${VECTOR_TEST_COMPAT_UNSET:-localhost,127.0.0.1}"]),
            false,
        ),
        (json!("SECRET[backend.no_proxy]"), true),
        (json!(["SECRET[backend.no_proxy]"]), false),
    ] {
        let content = serde_yaml::to_string(&json!({"proxy": {"no_proxy": input}})).unwrap();
        let content = with_absent_env_var(&content);
        let builder = ConfigBuilderLoader::default()
            .interpolate_env(true)
            .secrets(HashMap::from([(
                "backend.no_proxy".into(),
                "localhost,127.0.0.1".into(),
            )]))
            .load_from_input(content.as_bytes(), crate::config::Format::Yaml)
            .unwrap_or_else(|errors| panic!("{input}: {errors:?}"));
        let no_proxy = &builder.global.proxy.no_proxy;
        for host in ["localhost", "127.0.0.1"] {
            assert_eq!(no_proxy.matches(host), matches_hosts, "{input}: {host}");
        }
        assert!(!no_proxy.matches("example.com"), "{input}");
        assert_eq!(
            no_proxy.matches("localhost,127.0.0.1"),
            !matches_hosts,
            "{input}"
        );
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
            name: "interpolated environment boolean shorthand",
            input: json!("${VECTOR_TEST_COMPAT_UNSET:-false}"),
            enabled: Some(false),
        },
        Case {
            name: "interpolated secret boolean shorthand",
            input: json!("SECRET[backend.healthcheck]"),
            enabled: Some(false),
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
    const CONFIG: &str = indoc! {r"
        type: demo_logs
        format: json
        count: ${VECTOR_TEST_COMPAT_UNSET:-42}
        batch_interval: ${VECTOR_TEST_COMPAT_UNSET:-1.5}
        log_namespace: ${VECTOR_TEST_COMPAT_UNSET:-true}
    "};

    let directory = tempfile::tempdir().unwrap();
    write_files(directory.path(), &[("sources/demo.yaml", CONFIG)]);
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
    const CONFIG: &str = indoc! {r"
        name: event count
        outputs:
          - extract_from: transform
            expected_event_count: @COUNT@
    "};

    for (name, count) in [
        ("environment", "${VECTOR_TEST_COMPAT_UNSET:-42}"),
        ("secret", "SECRET[backend.count]"),
    ]
    .into_iter()
    .flat_map(|(name, count)| [(name, count.to_owned()), (name, format!("'{count}'"))])
    {
        let directory = tempfile::tempdir().unwrap();
        let input = CONFIG.replace("@COUNT@", &count);
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
    const SOURCE_TYPE: &str = indoc! {r"
        sources:
          demo:
            type: demo_logs
    "};
    const SOURCE_FORMAT: &str = indoc! {r"
        sources:
          demo:
            format: json
    "};
    const FRAGMENT: &str = indoc! {r"
        sources:
          demo:
            @FIELD@: @VALUE@
    "};

    struct Case {
        name: &'static str,
        field: &'static str,
        first: &'static str,
        second: &'static str,
        expected: Result<Value, &'static str>,
    }

    for case in [
        Case {
            name: "equal integers merge",
            field: "count",
            first: "43",
            second: "43",
            expected: Ok(json!(43)),
        },
        Case {
            name: "environment integer merges with literal",
            field: "count",
            first: "43",
            second: "${VECTOR_TEST_COMPAT_UNSET:-43}",
            expected: Ok(json!(43)),
        },
        Case {
            name: "secret integer merges with literal",
            field: "count",
            first: "SECRET[backend.count]",
            second: "43",
            expected: Ok(json!(43)),
        },
        Case {
            name: "environment float merges with literal",
            field: "interval",
            first: "2.5",
            second: "${VECTOR_TEST_COMPAT_UNSET:-2.5}",
            expected: Ok(json!(2.5)),
        },
        Case {
            name: "environment boolean merges with literal",
            field: "log_namespace",
            first: "true",
            second: "${VECTOR_TEST_COMPAT_UNSET:-true}",
            expected: Ok(json!(true)),
        },
        Case {
            name: "integer conflicts with boolean",
            field: "count",
            first: "1",
            second: "true",
            expected: Err("Incompatible types"),
        },
        Case {
            name: "integer conflicts with float",
            field: "interval",
            first: "1.5",
            second: "2",
            expected: Err("Incompatible types"),
        },
        Case {
            name: "array conflicts with string",
            field: "lines",
            first: "[first]",
            second: "second",
            expected: Err("Incompatible types"),
        },
        Case {
            name: "invalid interpolation cannot be hidden",
            field: "count",
            first: "${VECTOR_TEST_COMPAT_UNSET:-invalid}",
            second: "43",
            expected: Err("Expected integer at 'sources.demo.count'"),
        },
        Case {
            name: "quoted interpolation merges with integer",
            field: "count",
            first: "'${VECTOR_TEST_COMPAT_UNSET:-43}'",
            second: "43",
            expected: Ok(json!(43)),
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
                &[("type.yaml", SOURCE_TYPE), ("format.yaml", SOURCE_FORMAT)],
            );
            for (index, value) in fragments.iter().enumerate() {
                let input = FRAGMENT
                    .replace("@FIELD@", case.field)
                    .replace("@VALUE@", value);
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
            if let Ok(expected) = &case.expected {
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
                        .any(|error| error.contains(case.expected.as_ref().unwrap_err())),
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
    const CONFIG: &str = indoc! {r"
        sources:
          demo:
            type: demo_logs
            format: json
            count: 43
    "};

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
        write_files(
            directory.path(),
            &[("first.yaml", CONFIG), ("second.yaml", CONFIG)],
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
    const PARENT: &str = indoc! {r"
        inline:
          retained: true
    "};
    const INVALID_CONFIG: &str = "invalid: [yaml";
    const REQUIRED_ENVIRONMENT: &str = "value: ${VECTOR_TEST_COMPAT_UNSET:?must not be read}";
    const SECRET_REFERENCE: &str = "value: SECRET[missing.shadowed]";
    const NESTED_FILE: &str = "from_file: true";

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
            content: INVALID_CONFIG,
            expected: json!({"retained": true}),
        },
        Case {
            name: "required environment in shadowed folder",
            nested_path: "inline/ignored.yaml",
            content: REQUIRED_ENVIRONMENT,
            expected: json!({"retained": true}),
        },
        Case {
            name: "secret in shadowed folder",
            nested_path: "inline/ignored.yaml",
            content: SECRET_REFERENCE,
            expected: json!({"retained": true}),
        },
        Case {
            name: "nested file still contributes",
            nested_path: "inline.yaml",
            content: NESTED_FILE,
            expected: json!({"retained": true, "from_file": true}),
        },
    ] {
        let directory = tempfile::tempdir().unwrap();
        write_files(
            directory.path(),
            &[
                ("transforms/parent.yaml", PARENT),
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
    const YAML_PARENT: &str = indoc! {r"
        child:
          inline: true
    "};
    const JSON_PARENT: &str = r#"{"other":true}"#;
    const NESTED: &str = indoc! {r"
        nested: true
    "};

    let directory = tempfile::tempdir().unwrap();
    write_files(
        directory.path(),
        &[
            ("transforms/parent.yaml", YAML_PARENT),
            ("transforms/parent.json", JSON_PARENT),
            ("transforms/parent/child/disk.yaml", NESTED),
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

    // Parse-first loading requires syntactically valid input before substitution.
    // Quoted numeric and boolean placeholders are coerced using the field schema.
    for case in [
        Case {
            name: "YAML plain placeholders and a field alias",
            format: Format::Yaml,
            template: "sources:\n  demo:\n    type: demo_logs\n    format: json\n    count: @COUNT@\n    batch_interval: @INTERVAL@\n    log_namespace: @BOOL@\n",
            accepted: true,
        },
        Case {
            name: "YAML quoted placeholders",
            format: Format::Yaml,
            template: "sources:\n  demo:\n    type: demo_logs\n    format: json\n    count: '@COUNT@'\n    interval: '@INTERVAL@'\n    log_namespace: '@BOOL@'\n",
            accepted: true,
        },
        Case {
            name: "TOML unquoted placeholders",
            format: Format::Toml,
            template: "[sources.demo]\ntype = 'demo_logs'\nformat = 'json'\ncount = @COUNT@\ninterval = @INTERVAL@\nlog_namespace = @BOOL@\n",
            accepted: false,
        },
        Case {
            name: "TOML quoted placeholders",
            format: Format::Toml,
            template: "[sources.demo]\ntype = 'demo_logs'\nformat = 'json'\ncount = '@COUNT@'\ninterval = '@INTERVAL@'\nlog_namespace = '@BOOL@'\n",
            accepted: true,
        },
        Case {
            name: "JSON unquoted placeholders",
            format: Format::Json,
            template: r#"{"sources":{"demo":{"type":"demo_logs","format":"json","count":@COUNT@,"interval":@INTERVAL@,"log_namespace":@BOOL@}}}"#,
            accepted: false,
        },
        Case {
            name: "JSON quoted placeholders",
            format: Format::Json,
            template: r#"{"sources":{"demo":{"type":"demo_logs","format":"json","count":"@COUNT@","interval":"@INTERVAL@","log_namespace":"@BOOL@"}}}"#,
            accepted: true,
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
            count: '42'
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

#[test]
#[cfg(feature = "sources-demo_logs")]
fn yaml_scalar_substitutions_match_literal_values() {
    const CONFIG: &str = indoc! {r"
        sources:
          demo:
            type: demo_logs
            format: json
            count: @COUNT@
            log_namespace: @NAMESPACE@
        proxy:
          enabled: @ENABLED@
    "};

    for (count, enabled, namespace) in [
        ("0x2", "TRUE", "~"),
        ("0o2", "True", ""),
        ("0b10", "FALSE", "Null"),
        ("+0x2", "False", "NULL"),
    ] {
        let literal = CONFIG
            .replace("@COUNT@", count)
            .replace("@ENABLED@", enabled)
            .replace("@NAMESPACE@", namespace);
        let expected = ConfigBuilderLoader::default()
            .load_from_input(literal.as_bytes(), Format::Yaml)
            .unwrap();
        let expected = serde_json::to_value(expected).unwrap();
        for input in [
            CONFIG
                .replace(
                    "@COUNT@",
                    &format!("${{VECTOR_TEST_COMPAT_UNSET:-{count}}}"),
                )
                .replace(
                    "@ENABLED@",
                    &format!("${{VECTOR_TEST_COMPAT_UNSET:-{enabled}}}"),
                )
                .replace(
                    "@NAMESPACE@",
                    &format!("${{VECTOR_TEST_COMPAT_UNSET:-{namespace}}}"),
                ),
            CONFIG
                .replace("@COUNT@", "SECRET[backend.count]")
                .replace("@ENABLED@", "SECRET[backend.enabled]")
                .replace("@NAMESPACE@", "SECRET[backend.namespace]"),
        ] {
            let input = with_absent_env_var(&input);
            let actual = ConfigBuilderLoader::default()
                .interpolate_env(true)
                .secrets(HashMap::from([
                    ("backend.count".into(), count.into()),
                    ("backend.enabled".into(), enabled.into()),
                    ("backend.namespace".into(), namespace.into()),
                ]))
                .load_from_input(input.as_bytes(), Format::Yaml)
                .unwrap_or_else(|errors| panic!("{count} / {enabled} / {namespace:?}: {errors:?}"));
            assert_eq!(serde_json::to_value(actual).unwrap(), expected);
        }
    }
}

#[test]
#[cfg(feature = "sources-demo_logs")]
fn toml_numeric_substitutions_match_literal_values() {
    const CONFIG: &str = indoc! {r#"
        [sources.demo]
        type = "demo_logs"
        format = "json"
        count = @COUNT@
        interval = @INTERVAL@
    "#};

    for (count, interval) in [
        ("1_000", "1_000.5"),
        ("0xFF_FF", "1.0_5"),
        ("0b1_0", "1e1_0"),
    ] {
        let literal = CONFIG
            .replace("@COUNT@", count)
            .replace("@INTERVAL@", interval);
        let expected = ConfigBuilderLoader::default()
            .load_from_input(literal.as_bytes(), Format::Toml)
            .unwrap();
        let expected = serde_json::to_value(expected).unwrap();
        for input in [
            CONFIG
                .replace(
                    "@COUNT@",
                    &format!(r#""${{VECTOR_TEST_COMPAT_UNSET:-{count}}}""#),
                )
                .replace(
                    "@INTERVAL@",
                    &format!(r#""${{VECTOR_TEST_COMPAT_UNSET:-{interval}}}""#),
                ),
            CONFIG
                .replace("@COUNT@", r#""SECRET[backend.count]""#)
                .replace("@INTERVAL@", r#""SECRET[backend.interval]""#),
        ] {
            let input = with_absent_env_var(&input);
            let actual = ConfigBuilderLoader::default()
                .interpolate_env(true)
                .secrets(HashMap::from([
                    ("backend.count".into(), count.into()),
                    ("backend.interval".into(), interval.into()),
                ]))
                .load_from_input(input.as_bytes(), Format::Toml)
                .unwrap_or_else(|errors| panic!("{count} / {interval}: {errors:?}"));
            assert_eq!(serde_json::to_value(actual).unwrap(), expected);
        }
    }
}

#[test]
#[cfg(feature = "sources-fluent")]
fn fluent_defaults_to_tcp_before_coercing_its_fields() {
    const CONFIG: &str = indoc! {r"
        sources:
          incoming:
            type: fluent
            address: 127.0.0.1:24224
            connection_limit: ${VECTOR_TEST_COMPAT_UNSET:-2}
    "};
    let config = with_absent_env_var(CONFIG);
    let builder = ConfigBuilderLoader::default()
        .interpolate_env(true)
        .load_from_input(config.as_bytes(), crate::config::Format::Yaml)
        .unwrap();
    let source = &serde_json::to_value(builder).unwrap()["sources"]["incoming"];
    assert_eq!(source["mode"], "tcp");
    assert_eq!(source["connection_limit"], 2);
}

#[test]
#[cfg(feature = "sources-http_server")]
fn http_auth_defaults_to_basic_after_secret_substitution() {
    const CONFIG: &str = indoc! {r"
        sources:
          web:
            type: http_server
            address: 127.0.0.1:8080
            auth:
              username: user
              password: SECRET[backend.password]
    "};
    let builder = ConfigBuilderLoader::default()
        .secrets(HashMap::from([("backend.password".into(), "pass".into())]))
        .load_from_input(CONFIG.as_bytes(), crate::config::Format::Yaml)
        .unwrap();
    let auth = &serde_json::to_value(builder).unwrap()["sources"]["web"]["auth"];
    assert_eq!(
        auth,
        &json!({"strategy": "basic", "username": "user", "password": "pass"})
    );
}

#[test]
#[cfg(feature = "sources-static_metrics")]
fn histogram_bounds_preserve_special_strings() {
    const CONFIG: &str = indoc! {r#"
        sources:
          fixed:
            type: static_metrics
            metrics:
              - name: latency
                kind: absolute
                tags: {}
                value:
                  aggregated_histogram:
                    buckets:
                      - upper_limit: "inf"
                        count: 1
                      - upper_limit: "-inf"
                        count: 2
                      - upper_limit: "NaN"
                        count: 3
                      - upper_limit: 1.5
                        count: 4
                    count: 10
                    sum: 0
    "#};
    let builder = ConfigBuilderLoader::default()
        .load_from_input(CONFIG.as_bytes(), crate::config::Format::Yaml)
        .unwrap();
    let value = serde_json::to_value(builder).unwrap();
    let buckets =
        &value["sources"]["fixed"]["metrics"][0]["value"]["aggregated_histogram"]["buckets"];
    assert_eq!(
        buckets,
        &json!([
            {"upper_limit": "inf", "count": 1},
            {"upper_limit": "-inf", "count": 2},
            {"upper_limit": "NaN", "count": 3},
            {"upper_limit": 1.5, "count": 4}
        ])
    );
}
