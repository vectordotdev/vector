//! Exercise ignored-field diagnostics through the real component schemas and configuration loader.

#![cfg(all(
    feature = "sources-file",
    feature = "sources-http_server",
    feature = "sinks-http"
))]

use std::io::{Read as _, Seek as _, SeekFrom};

use indoc::indoc;
use serde_json::Value;
use tracing_subscriber::{Layer as _, layer::SubscriberExt as _};

use crate::config::{
    ConfigBuilder, Format,
    loading::{
        ComponentHint, ConfigBuilderLoader, Loader as _, Process as _, ignored_fields, load_builder,
    },
};

const CONFIG: &str = indoc! {r#"
    sources:
      files:
        type: file
        include: ["/tmp/vector-ignored-fields/*.log"]
        fingerprinting:
          strategy: device_and_inode
          ignored_header_bytes: 0
          unknown_option: 42
      http:
        type: http_server
        address: "127.0.0.1:18080"
        auth:
          strategy: bearer
          token: "BEARER_SECRET"
          username: "UNUSED_USERNAME"
          password: "UNUSED_PASSWORD"
    sinks:
      out:
        type: http
        inputs: [files, http]
        uri: "http://127.0.0.1:18081/"
        encoding:
          codec: json
        request:
          retry_atempts: 3
          headers:
            X-Token: "HEADER_SECRET"
"#};

#[test]
fn supported_defaults_aliases_and_maps_do_not_warn() {
    const CONFIG: &str = indoc! {r#"
        timezone: local
        sources:
          files:
            type: file
            include: ["/tmp/vector-ignored-fields/*.log"]
            fingerprinting:
              strategy: checksum
              ignored_header_bytes: 0
          http:
            type: http_server
            address: "127.0.0.1:18080"
            auth:
              username: alice
              password: "BASIC_SECRET"
        sinks:
          out:
            type: http
            inputs: [files, http]
            uri: "http://127.0.0.1:18081/"
            encoding:
              codec: json
            request:
              retry_attempts: 3
              headers:
                X-Token: "HEADER_SECRET"
    "#};
    let value: Value = serde_yaml::from_str(CONFIG).unwrap();
    let _: ConfigBuilder = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(
        ignored_fields::find(value.as_object().unwrap(), None),
        Vec::<String>::new()
    );
}

fn with_logs<T>(f: impl FnOnce() -> T) -> (T, String) {
    let mut buffer = tempfile::tempfile().unwrap();
    let formatter = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .without_time()
        .with_writer(buffer.try_clone().unwrap());
    let subscriber = tracing_subscriber::registry().with(
        tracing_limit::RateLimitedLayer::new(formatter)
            .with_default_limit(60)
            .with_filter(tracing_subscriber::filter::LevelFilter::WARN),
    );
    let result = tracing::subscriber::with_default(subscriber, f);
    let mut logs = String::new();
    buffer.seek(SeekFrom::Start(0)).unwrap();
    buffer.read_to_string(&mut logs).unwrap();
    (result, logs)
}

#[test]
fn loading_warns_without_changing_values_or_flooding_logs() {
    let expected: ConfigBuilder = serde_yaml::from_str(CONFIG).unwrap();
    let expected = serde_json::to_value(expected).unwrap();
    let ((), logs) = with_logs(|| {
        for _ in 0..5 {
            let builder = ConfigBuilderLoader::default()
                .interpolate_env(false)
                .load_from_input(CONFIG.as_bytes(), Format::Yaml)
                .unwrap();
            assert_eq!(serde_json::to_value(&builder).unwrap(), expected);
            builder.build().unwrap();
        }
    });
    assert_eq!(logs.matches("DEPRECATED:").count(), 5, "{logs}");
    assert!(logs.contains("is being suppressed"), "{logs}");
    for path in [
        "/sinks/out/request/retry_atempts",
        "/sources/files/fingerprinting/ignored_header_bytes",
        "/sources/files/fingerprinting/unknown_option",
        "/sources/http/auth/password",
        "/sources/http/auth/username",
    ] {
        assert_eq!(logs.matches(path).count(), 1, "{logs}");
    }
    for secret in [
        "BEARER_SECRET",
        "UNUSED_USERNAME",
        "UNUSED_PASSWORD",
        "HEADER_SECRET",
    ] {
        assert!(
            !logs.contains(secret),
            "a configuration value appeared in the warning"
        );
    }
}

#[test]
fn invalid_configurations_keep_their_error_without_deprecation_warnings() {
    const CONFIG: &str = indoc! {r#"
        sources:
          http:
            type: http_server
            address: "127.0.0.1:18080"
            auth:
              strategy: bearer
              username: alice
    "#};
    let (result, logs) = with_logs(|| {
        ConfigBuilderLoader::default()
            .interpolate_env(false)
            .load_from_input(CONFIG.as_bytes(), Format::Yaml)
    });
    assert!(
        result
            .unwrap_err()
            .join(" ")
            .contains("missing field `token`")
    );
    assert!(!logs.contains("DEPRECATED:"), "{logs}");
}

#[test]
fn component_and_appended_warnings_reach_build_with_warnings() {
    let value: Value = serde_yaml::from_str(CONFIG).unwrap();
    let mut loader = ConfigBuilderLoader::default();
    for (name, hint) in [
        ("sources", ComponentHint::Source),
        ("sinks", ComponentHint::Sink),
    ] {
        loader
            .merge(value[name].as_object().unwrap().clone(), Some(hint))
            .unwrap();
    }
    let mut builder = ConfigBuilder::default();
    builder.append(loader.take()).unwrap();
    let expected =
        ignored_fields::warnings(&ignored_fields::find(value.as_object().unwrap(), None));
    let (_, mut warnings) = builder.build_with_warnings().unwrap();
    warnings.sort();
    assert_eq!(warnings, expected);
}

#[test]
fn raw_input_loading_preserves_values_and_collects_warnings_in_all_formats() {
    let value: Value = serde_yaml::from_str(CONFIG).unwrap();
    let expected = serde_json::to_value(
        crate::config::load::<_, ConfigBuilder>(CONFIG.as_bytes(), Format::Yaml).unwrap(),
    )
    .unwrap();
    for (format, input) in [
        (Format::Yaml, CONFIG.to_owned()),
        (Format::Json, serde_json::to_string(&value).unwrap()),
        (Format::Toml, toml::to_string(&value).unwrap()),
    ] {
        let builder = load_builder(input.as_bytes(), format).unwrap();
        assert_eq!(serde_json::to_value(&builder).unwrap(), expected);
        let (_, warnings) = builder.build_with_warnings().unwrap();
        assert_eq!(warnings.len(), 5);
    }
}

#[test]
fn raw_input_loading_preserves_deserialization_errors() {
    const CONFIGS: &[&str] = &[
        "null",
        "[]",
        "42",
        "data_dir: []",
        "sources: {bad: {type: does_not_exist}}",
    ];
    for config in CONFIGS {
        let expected =
            crate::config::load::<_, ConfigBuilder>(config.as_bytes(), Format::Yaml).unwrap_err();
        assert_eq!(
            load_builder(config.as_bytes(), Format::Yaml).unwrap_err(),
            expected
        );
    }
}

#[cfg(feature = "api")]
#[test]
fn raw_input_loading_preserves_explicit_null_instead_of_merging_defaults() {
    const CONFIG: &str = "api: {address: null}";
    let builder = load_builder(CONFIG.as_bytes(), Format::Yaml).unwrap();
    assert_eq!(builder.api.address, None);
    assert!(builder.loading_warnings.is_empty());
}
