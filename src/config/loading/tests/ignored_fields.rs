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
    loading::{ConfigBuilderLoader, ignored_fields},
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
        buffer:
          max_events: 100
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
    let load = |input: &str| {
        let expected: ConfigBuilder = serde_yaml::from_str(input).unwrap();
        let builder = ConfigBuilderLoader::default()
            .interpolate_env(false)
            .load_from_input(input.as_bytes(), Format::Yaml)
            .unwrap();
        assert_eq!(
            serde_json::to_value(&builder).unwrap(),
            serde_json::to_value(expected).unwrap()
        );
        builder.build().unwrap();
    };
    let updated = CONFIG.replace("retry_atempts", "timeout_sec");
    let ((), logs) = with_logs(|| {
        for _ in 0..5 {
            load(CONFIG);
        }
        load(&updated);
        load(&updated);
    });
    assert_eq!(logs.matches("DEPRECATED:").count(), 10, "{logs}");
    for (path, count) in [
        ("/sinks/out/request/retry_atempts", 1),
        ("/sinks/out/request/timeout_sec", 1),
        ("/sources/files/fingerprinting/ignored_header_bytes", 2),
        ("/sources/files/fingerprinting/unknown_option", 2),
        ("/sources/http/auth/password", 2),
        ("/sources/http/auth/username", 2),
    ] {
        assert_eq!(logs.matches(path).count(), count, "{logs}");
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
