//! Exercise ignored-field diagnostics through the real component schemas and configuration loader.

#![cfg(all(
    feature = "sources-file",
    feature = "sources-http_server",
    feature = "sinks-http"
))]

use std::{
    io::{self, Write},
    sync::{Arc, Mutex},
};

use indoc::indoc;
use serde_json::Value;
use tracing_subscriber::{Layer as _, layer::SubscriberExt as _};

use crate::config::{
    ConfigBuilder, Format,
    loading::{ComponentHint, ConfigBuilderLoader, ignored_fields},
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
fn finds_unused_fields_in_custom_enums_and_flattened_structs() {
    let value: Value = serde_yaml::from_str(CONFIG).unwrap();
    let paths = ignored_fields::find(value.as_object().unwrap(), None);
    assert_eq!(
        paths,
        [
            "/sinks/out/request/retry_atempts",
            "/sources/files/fingerprinting/ignored_header_bytes",
            "/sources/files/fingerprinting/unknown_option",
            "/sources/http/auth/password",
            "/sources/http/auth/username",
        ]
    );
}

#[test]
fn component_directory_fields_include_their_scope() {
    let value: Value = serde_yaml::from_str(CONFIG).unwrap();
    let paths = ignored_fields::find(
        value["sources"].as_object().unwrap(),
        Some(ComponentHint::Source),
    );
    assert_eq!(paths.len(), 4);
    assert!(paths.iter().all(|path| path.starts_with("/sources/")));
}

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

#[derive(Clone)]
struct Capture(Arc<Mutex<Vec<u8>>>);

impl Write for Capture {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn load_with_logs(config: &str) -> (Result<ConfigBuilder, Vec<String>>, String) {
    let buffer = Arc::new(Mutex::new(Vec::new()));
    let writer = Capture(Arc::clone(&buffer));
    let formatter = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .without_time()
        .with_writer(move || writer.clone());
    let subscriber = tracing_subscriber::registry().with(
        tracing_limit::RateLimitedLayer::new(formatter)
            .with_filter(tracing_subscriber::filter::LevelFilter::WARN),
    );
    let result = tracing::subscriber::with_default(subscriber, || {
        ConfigBuilderLoader::default()
            .interpolate_env(false)
            .load_from_input(config.as_bytes(), Format::Yaml)
    });
    let logs = String::from_utf8(buffer.lock().unwrap().clone()).unwrap();
    (result, logs)
}

#[test]
fn loading_warns_and_preserves_configuration_without_logging_values() {
    let (builder, logs) = load_with_logs(CONFIG);
    let expected: ConfigBuilder = serde_yaml::from_str(CONFIG).unwrap();
    assert_eq!(
        serde_json::to_value(builder.unwrap()).unwrap(),
        serde_json::to_value(expected).unwrap()
    );
    assert_eq!(logs.matches("DEPRECATED:").count(), 5, "{logs}");
    assert!(logs.contains("/sources/http/auth/password"), "{logs}");
    assert!(logs.contains("/sinks/out/request/retry_atempts"), "{logs}");
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
    let (result, logs) = load_with_logs(CONFIG);
    assert!(
        result
            .unwrap_err()
            .join(" ")
            .contains("missing field `token`")
    );
    assert!(!logs.contains("DEPRECATED:"), "{logs}");
}
