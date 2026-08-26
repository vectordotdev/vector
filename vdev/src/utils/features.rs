use std::{
    collections::{BTreeSet, HashMap},
    env,
    ffi::OsStr,
    fs,
    path::Path,
    process::Command,
    sync::LazyLock,
};

use anyhow::{Context, Result, bail};
use regex::Regex;
use serde::Deserialize;
use serde_json::Value;

use crate::utils::{cargo::CargoToml, paths::find_repo_root};

type ComponentMap = HashMap<String, Component>;

// Use a BTree to keep the results in sorted order
type FeatureSet = BTreeSet<String>;

// Feature extraction rules and special cases. Keep additions here so the supported surface is
// visible without reading the traversal code below.
struct NestedFeatureRule {
    parent_key: Option<&'static str>,
    key: &'static str,
    value: &'static str,
    feature: &'static str,
}

const NESTED_FEATURE_RULES: &[NestedFeatureRule] = &[
    NestedFeatureRule {
        parent_key: None,
        key: "codec",
        value: "otlp",
        feature: "codecs-opentelemetry",
    },
    NestedFeatureRule {
        parent_key: None,
        key: "codec",
        value: "parquet",
        feature: "codecs-parquet",
    },
    NestedFeatureRule {
        parent_key: None,
        key: "codec",
        value: "syslog",
        feature: "codecs-syslog",
    },
    NestedFeatureRule {
        parent_key: Some("auth"),
        key: "strategy",
        value: "aws",
        feature: "aws-core",
    },
];
const ALWAYS_COMPILED_FEATURES: &[&str] = &[
    "enrichment-tables-file",
    "secrets-directory",
    "secrets-exec",
    "secrets-file",
    "secrets-test",
];
const VRL_FUNCTION_FEATURES: &[&str] = &[
    "vrl-functions-crypto",
    "vrl-functions-env",
    "vrl-functions-network",
    "vrl-functions-system",
];
const DYNAMIC_SELECTOR_KEYS: &[&str] = &["type", "codec", "sasl.mechanism", "sasl.mechanisms"];
const VRL_DISCRIMINATORS: &[(&str, &str)] = &[("type", "vrl"), ("codec", "vrl")];
const VRL_PROGRAM_KEYS: &[&str] = &["source", "value"];
const TRANSFORM_PROGRAM_KEYS: &[&str] = &["source", "condition"];
const KAFKA_OPTIONS_KEY: &str = "librdkafka_options";
const KAFKA_GSSAPI_KEYS: &[&str] = &["sasl.mechanism", "sasl.mechanisms"];
const KAFKA_GSSAPI_VALUE: &str = "GSSAPI";
const GSSAPI_FEATURE: &str = "gssapi";
const GSSAPI_VENDORED_FEATURE: &str = "gssapi-vendored";
const AUTH_KEY: &str = "auth";
const STRATEGY_KEY: &str = "strategy";
const CUSTOM_STRATEGY: &str = "custom";
const CONDITION_KEY: &str = "condition";
const SOURCE_KEY: &str = "source";
const DNSTAP_FUNCTION: &str = "parse_dnstap";
const DNSTAP_FEATURE: &str = "sources-dnstap";
const CARGO_CONFIG_FILENAMES: &[&str] = &["config.toml", "config"];

const UNSUPPORTED_INTERPOLATION: &str = "cargo vdev feature selection does not support environment or secret interpolation in feature selectors or VRL programs; use `cargo run` instead";
const UNSUPPORTED_PROVIDER: &str = "cargo vdev feature selection does not support configuration providers; use `cargo run` instead";
static ENV_INTERPOLATION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\$\$|\$([[:word:].]+)|\$\{([[:word:].]+)(?:(:?-|:?\?)([^}]*))?\}")
        .expect("environment interpolation regex must compile")
});
static SECRET_INTERPOLATION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"SECRET\[([[:word:]\-]+)\.([[:word:].\-/]+)\]")
        .expect("secret interpolation regex must compile")
});

/// Partial configuration used to discover the features needed to compile Vector.
///
/// This cannot use `vector::config::ConfigBuilder`: deserializing that type requires the component
/// features that this pass is responsible for discovering.
#[derive(Deserialize)]
struct FeatureConfig {
    api: Option<Value>,
    provider: Option<Value>,

    #[serde(default)]
    enrichment_tables: ComponentMap,
    #[serde(default)]
    secret: ComponentMap,
    #[serde(default)]
    sources: ComponentMap,
    #[serde(default)]
    transforms: ComponentMap,
    #[serde(default)]
    sinks: ComponentMap,

    #[serde(flatten)]
    other: HashMap<String, Value>,
}

#[derive(Deserialize)]
struct Component {
    r#type: String,

    #[serde(flatten)]
    options: HashMap<String, Value>,
}

pub fn load_and_extract(filename: &Path) -> Result<Vec<String>> {
    let config = fs::read_to_string(filename)
        .with_context(|| format!("failed to read {}", filename.display()))?;

    let config: FeatureConfig = match filename
        .extension()
        .and_then(OsStr::to_str)
        .map(str::to_lowercase)
        .as_deref()
    {
        None => bail!("Invalid filename {}, no extension", filename.display()),
        Some("json") => serde_json::from_str(&config)?,
        Some("toml") => toml::from_str(&config)?,
        Some("yaml" | "yml") => serde_yaml::from_str(&config)?,
        Some(_) => bail!("Invalid filename {}, unknown extension", filename.display()),
    };

    let declared_features = CargoToml::load_from(&find_repo_root()?.join("Cargo.toml"))?
        .features
        .into_keys()
        .collect();

    from_config(&config, &declared_features)
}

fn from_config(config: &FeatureConfig, declared_features: &FeatureSet) -> Result<Vec<String>> {
    if config.provider.is_some() {
        bail!(UNSUPPORTED_PROVIDER);
    }
    validate_feature_selectors(config)?;

    let mut features = FeatureSet::default();
    add_option(&mut features, "api", config.api.as_ref());

    get_features(
        &mut features,
        "enrichment-tables",
        &config.enrichment_tables,
        declared_features,
    );
    get_features(&mut features, "secrets", &config.secret, declared_features);
    get_features(&mut features, "sources", &config.sources, declared_features);
    get_features(
        &mut features,
        "transforms",
        &config.transforms,
        declared_features,
    );
    get_features(&mut features, "sinks", &config.sinks, declared_features);

    let mut uses_vrl = !config.transforms.is_empty();

    for section in [
        &config.enrichment_tables,
        &config.secret,
        &config.sources,
        &config.transforms,
        &config.sinks,
    ] {
        for component in section.values() {
            for (key, value) in &component.options {
                get_nested_features(&mut features, &mut uses_vrl, Some(key.as_str()), value);
            }
        }
    }
    for (key, value) in &config.other {
        get_nested_features(&mut features, &mut uses_vrl, Some(key.as_str()), value);
    }

    if uses_vrl {
        features.extend(
            VRL_FUNCTION_FEATURES
                .iter()
                .map(|feature| (*feature).into()),
        );
    }

    for feature in ALWAYS_COMPILED_FEATURES {
        features.remove(*feature);
    }

    Ok(features.into_iter().collect())
}

fn validate_feature_selectors(config: &FeatureConfig) -> Result<()> {
    for (section, contains_transforms) in [
        (&config.enrichment_tables, false),
        (&config.secret, false),
        (&config.sources, false),
        (&config.transforms, true),
        (&config.sinks, false),
    ] {
        for component in section.values() {
            if is_dynamic(&component.r#type)
                || component.options.iter().any(|(key, value)| {
                    (contains_transforms
                        && TRANSFORM_PROGRAM_KEYS.contains(&key.as_str())
                        && contains_dynamic_value(value))
                        || has_dynamic_feature_input(Some(key.as_str()), value)
                })
            {
                bail!(UNSUPPORTED_INTERPOLATION);
            }
        }
    }
    if config
        .other
        .iter()
        .any(|(key, value)| has_dynamic_feature_input(Some(key.as_str()), value))
    {
        bail!(UNSUPPORTED_INTERPOLATION);
    }
    Ok(())
}

fn has_dynamic_feature_input(parent_key: Option<&str>, value: &Value) -> bool {
    match value {
        Value::Array(values) => values
            .iter()
            .any(|value| has_dynamic_feature_input(parent_key, value)),
        Value::Object(object) => {
            DYNAMIC_SELECTOR_KEYS
                .iter()
                .filter_map(|key| object.get(*key).and_then(Value::as_str))
                .any(is_dynamic)
                || (parent_key == Some(AUTH_KEY)
                    && object
                        .get(STRATEGY_KEY)
                        .and_then(Value::as_str)
                        .is_some_and(is_dynamic))
                || object
                    .get(CONDITION_KEY)
                    .is_some_and(contains_dynamic_value)
                || (VRL_DISCRIMINATORS
                    .iter()
                    .any(|(key, value)| object.get(*key).and_then(Value::as_str) == Some(*value))
                    && VRL_PROGRAM_KEYS
                        .iter()
                        .filter_map(|key| object.get(*key))
                        .any(contains_dynamic_value))
                || (parent_key == Some(AUTH_KEY)
                    && object.get(STRATEGY_KEY).and_then(Value::as_str) == Some(CUSTOM_STRATEGY)
                    && object.get(SOURCE_KEY).is_some_and(contains_dynamic_value))
                || object
                    .iter()
                    .any(|(key, value)| has_dynamic_feature_input(Some(key.as_str()), value))
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => false,
    }
}

fn contains_dynamic_value(value: &Value) -> bool {
    match value {
        Value::Array(values) => values.iter().any(contains_dynamic_value),
        Value::Object(object) => object.values().any(contains_dynamic_value),
        Value::String(value) => is_dynamic(value),
        Value::Null | Value::Bool(_) | Value::Number(_) => false,
    }
}

fn is_dynamic(value: &str) -> bool {
    ENV_INTERPOLATION
        .captures_iter(value)
        .any(|captures| captures.get(1).is_some() || captures.get(2).is_some())
        || SECRET_INTERPOLATION.is_match(value)
}

fn add_option<T>(features: &mut FeatureSet, name: &str, field: Option<&T>) {
    if field.is_some() {
        features.insert(name.into());
    }
}

// Prefer the `<section>-<component type>` feature. Components that share an implementation use the
// longest declared underscore-delimited prefix, such as `sinks-humio` for `humio_metrics`.
fn get_features(
    features: &mut FeatureSet,
    key: &str,
    section: &ComponentMap,
    declared_features: &FeatureSet,
) {
    features.extend(
        section
            .values()
            .map(|component| component_feature(key, &component.r#type, declared_features)),
    );
}

fn component_feature(key: &str, component_type: &str, declared_features: &FeatureSet) -> String {
    let exact = format!("{key}-{component_type}");
    let mut prefix = component_type;

    loop {
        let candidate = format!("{key}-{prefix}");
        if declared_features.contains(&candidate) {
            return candidate;
        }
        let Some((shorter, _)) = prefix.rsplit_once('_') else {
            return exact;
        };
        prefix = shorter;
    }
}

fn get_nested_features(
    features: &mut FeatureSet,
    uses_vrl: &mut bool,
    parent_key: Option<&str>,
    value: &Value,
) {
    match value {
        Value::Array(values) => {
            for value in values {
                get_nested_features(features, uses_vrl, parent_key, value);
            }
        }
        Value::Object(object) => {
            for rule in NESTED_FEATURE_RULES {
                if rule
                    .parent_key
                    .is_none_or(|required| parent_key == Some(required))
                    && object.get(rule.key).and_then(Value::as_str) == Some(rule.value)
                {
                    features.insert(rule.feature.into());
                }
            }

            if parent_key == Some(KAFKA_OPTIONS_KEY)
                && KAFKA_GSSAPI_KEYS
                    .iter()
                    .filter_map(|key| object.get(*key).and_then(Value::as_str))
                    .any(|mechanism| mechanism.eq_ignore_ascii_case(KAFKA_GSSAPI_VALUE))
            {
                features.insert(kafka_gssapi_feature().into());
            }
            if object.contains_key(CONDITION_KEY)
                || VRL_DISCRIMINATORS
                    .iter()
                    .any(|(key, value)| object.get(*key).and_then(Value::as_str) == Some(*value))
                || (parent_key == Some(AUTH_KEY)
                    && object.get(STRATEGY_KEY).and_then(Value::as_str) == Some(CUSTOM_STRATEGY)
                    && object.contains_key(SOURCE_KEY))
            {
                *uses_vrl = true;
            }
            for (key, value) in object {
                get_nested_features(features, uses_vrl, Some(key.as_str()), value);
            }
        }
        Value::String(value) => {
            if uses_dnstap(value) {
                features.insert(DNSTAP_FEATURE.into());
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

fn uses_dnstap(mut value: &str) -> bool {
    while let Some(index) = value.find(DNSTAP_FUNCTION) {
        let (prefix, matched) = value.split_at(index);
        let suffix = matched
            .strip_prefix(DNSTAP_FUNCTION)
            .expect("find returned the start of the function name");
        let has_identifier_prefix = prefix
            .chars()
            .next_back()
            .is_some_and(|character| character.is_alphanumeric() || character == '_');
        let call = suffix.trim_start();

        if !has_identifier_prefix && (call.starts_with('(') || call.starts_with("!(")) {
            return true;
        }

        value = suffix;
    }

    false
}

fn kafka_gssapi_feature() -> &'static str {
    let configured_target = cargo_build_target();
    let host_target = rustc_host_target();
    let native_build = configured_target
        .as_ref()
        .is_none_or(|target| host_target.as_ref() == Some(target));
    let target_is_linux = configured_target
        .as_deref()
        .or(host_target.as_deref())
        .map_or(cfg!(target_os = "linux"), |target| {
            target.split('-').any(|part| part == "linux")
        });

    select_kafka_gssapi_feature(target_is_linux, native_build && system_sasl_available())
}

fn select_kafka_gssapi_feature(
    target_is_linux: bool,
    target_system_sasl_available: bool,
) -> &'static str {
    if target_is_linux && !target_system_sasl_available {
        GSSAPI_VENDORED_FEATURE
    } else {
        GSSAPI_FEATURE
    }
}

fn cargo_build_target() -> Option<String> {
    env::var("CARGO_BUILD_TARGET")
        .ok()
        .filter(|target| !target.is_empty())
        .or_else(configured_cargo_build_target)
}

fn configured_cargo_build_target() -> Option<String> {
    let mut directory = env::current_dir().ok()?;

    loop {
        for filename in CARGO_CONFIG_FILENAMES {
            let path = directory.join(".cargo").join(filename);
            let target = fs::read_to_string(path)
                .ok()
                .and_then(|config| toml::from_str::<toml::Value>(&config).ok())
                .and_then(|config| {
                    config
                        .get("build")?
                        .get("target")?
                        .as_str()
                        .map(str::to_owned)
                });
            if target.is_some() {
                return target;
            }
        }

        if !directory.pop() {
            return None;
        }
    }
}

fn rustc_host_target() -> Option<String> {
    let rustc = env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let output = Command::new(rustc).arg("-vV").output().ok()?;
    if !output.status.success() {
        return None;
    }

    String::from_utf8(output.stdout)
        .ok()?
        .lines()
        .find_map(|line| line.strip_prefix("host: ").map(str::to_owned))
}

fn system_sasl_available() -> bool {
    Command::new("pkg-config")
        .args(["--exists", "libsasl2"])
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path, sync::LazyLock};

    use super::{
        CargoToml, FeatureConfig, FeatureSet, from_config, kafka_gssapi_feature,
        select_kafka_gssapi_feature,
    };

    static DECLARED_FEATURES: LazyLock<FeatureSet> = LazyLock::new(|| {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("vdev must have a parent directory")
            .join("Cargo.toml");
        CargoToml::load_from(&manifest)
            .expect("workspace Cargo.toml must load")
            .features
            .into_keys()
            .collect()
    });

    fn features(config: &str) -> Vec<String> {
        from_config(
            &serde_yaml::from_str::<FeatureConfig>(config).expect("config must parse"),
            &DECLARED_FEATURES,
        )
        .expect("feature extraction must succeed")
    }

    #[test]
    fn derives_exact_or_shared_component_features() {
        let config = r"
sources:
  gcp:
    type: gcp_pubsub
  prometheus:
    type: prometheus_scrape
transforms:
  route:
    type: exclusive_route
sinks:
  databricks:
    type: databricks_zerobus
  chronicle:
    type: gcp_chronicle_unstructured
  humio_logs:
    type: humio_logs
  humio_metrics:
    type: humio_metrics
  influxdb_logs:
    type: influxdb_logs
  influxdb_metrics:
    type: influxdb_metrics
  sematext_logs:
    type: sematext_logs
  sematext_metrics:
    type: sematext_metrics
  splunk:
    type: splunk_hec_metrics
  websocket:
    type: websocket_server
";

        assert_eq!(
            features(config),
            [
                "sinks-databricks_zerobus",
                "sinks-gcp",
                "sinks-humio",
                "sinks-influxdb",
                "sinks-sematext",
                "sinks-splunk_hec",
                "sinks-websocket_server",
                "sources-gcp_pubsub",
                "sources-prometheus_scrape",
                "transforms-exclusive_route",
                "vrl-functions-crypto",
                "vrl-functions-env",
                "vrl-functions-network",
                "vrl-functions-system",
            ]
        );
    }

    #[test]
    fn extracts_top_level_feature_gates() {
        let config = r"
enrichment_tables:
  memory:
    type: memory
  geoip:
    type: geoip
  file:
    type: file
secret:
  aws:
    type: aws_secrets_manager
  file:
    type: file
";

        assert_eq!(
            features(config),
            [
                "enrichment-tables-geoip",
                "enrichment-tables-memory",
                "secrets-aws_secrets_manager",
            ]
        );
    }

    #[test]
    fn extracts_log_to_metric_feature() {
        assert_eq!(
            features("transforms:\n  metrics:\n    type: log_to_metric\n"),
            [
                "transforms-log_to_metric",
                "vrl-functions-crypto",
                "vrl-functions-env",
                "vrl-functions-network",
                "vrl-functions-system",
            ]
        );
    }

    #[test]
    fn extracts_nested_codec_and_enables_all_vrl_function_features() {
        let config = r#"
transforms:
  remap:
    type: remap
    source: |
      .message = "hello"
sinks:
  s3:
    type: aws_s3
    batch_encoding:
      codec: parquet
"#;

        assert_eq!(
            features(config),
            [
                "codecs-parquet",
                "sinks-aws_s3",
                "transforms-remap",
                "vrl-functions-crypto",
                "vrl-functions-env",
                "vrl-functions-network",
                "vrl-functions-system",
            ]
        );
    }

    #[test]
    fn enables_dnstap_for_both_vrl_call_variants() {
        for call in ["parse_dnstap(.message)", "parse_dnstap!(.message)"] {
            let config = format!(
                r"
transforms:
  remap:
    type: remap
    source: |
      .dns = {call}
"
            );

            assert!(features(&config).contains(&"sources-dnstap".into()));
        }
    }

    #[test]
    fn extracts_all_gated_codecs_and_aws_auth() {
        let config = r"
sources:
  socket:
    type: socket
    decoding:
      codec: syslog
sinks:
  http:
    type: http
    encoding:
      codec: otlp
    auth:
      strategy: aws
";

        assert_eq!(
            features(config),
            [
                "aws-core",
                "codecs-opentelemetry",
                "codecs-syslog",
                "sinks-http",
                "sources-socket",
            ]
        );
    }

    #[test]
    fn enables_available_gssapi_implementation_for_kafka() {
        for key in ["sasl.mechanism", "sasl.mechanisms"] {
            let config = format!(
                r"
sources:
  kafka:
    type: kafka
    librdkafka_options:
      {key}: GSSAPI
"
            );
            assert_eq!(
                features(&config),
                [
                    kafka_gssapi_feature().to_owned(),
                    "sources-kafka".to_owned(),
                ]
            );
        }

        assert_eq!(select_kafka_gssapi_feature(true, true), "gssapi");
        assert_eq!(select_kafka_gssapi_feature(true, false), "gssapi-vendored");
        assert_eq!(select_kafka_gssapi_feature(false, false), "gssapi");
    }

    #[test]
    fn enables_all_vrl_function_features_for_any_transform() {
        let config = "transforms:\n  transform:\n    type: dedupe\n";
        let features = features(config);

        for feature in [
            "vrl-functions-crypto",
            "vrl-functions-env",
            "vrl-functions-network",
            "vrl-functions-system",
        ] {
            assert!(
                features.iter().any(|candidate| candidate == feature),
                "any transform must enable {feature}"
            );
        }
    }

    #[test]
    fn enables_all_vrl_function_features_for_nested_vrl_configuration() {
        for config in [
            "sources:\n  input:\n    type: http_client\n    query:\n      host:\n        type: vrl\n        value: get_hostname!()\n",
            "sources:\n  input:\n    type: stdin\n    decoding:\n      codec: vrl\n      vrl:\n        source: get_hostname!()\n",
            "sources:\n  input:\n    type: http_server\n    auth:\n      strategy: custom\n      source: get_hostname!() == \"host\"\n",
            "tests:\n  - condition: .message == \"hello\"\n",
        ] {
            let features = features(config);

            for feature in [
                "vrl-functions-crypto",
                "vrl-functions-env",
                "vrl-functions-network",
                "vrl-functions-system",
            ] {
                assert!(
                    features.iter().any(|candidate| candidate == feature),
                    "nested VRL configuration must enable {feature}"
                );
            }
        }
    }

    #[test]
    fn rejects_configuration_providers() {
        let config = serde_yaml::from_str::<FeatureConfig>(
            "provider:\n  type: http\n  url: https://example.com/vector.yaml\n",
        )
        .expect("config must parse");
        let error =
            from_config(&config, &DECLARED_FEATURES).expect_err("configuration provider must fail");

        assert!(error.to_string().contains("use `cargo run` instead"));
    }

    #[test]
    fn rejects_interpolated_feature_selectors() {
        for config in [
            "sources:\n  input:\n    type: ${SOURCE_TYPE}\n",
            "sources:\n  input:\n    type: SECRET[features.source_type]\n",
            "sinks:\n  output:\n    type: aws_s3\n    encoding:\n      codec: ${CODEC}\n",
            "sinks:\n  output:\n    type: http\n    auth:\n      strategy: ${AUTH_STRATEGY}\n",
            "sources:\n  input:\n    type: kafka\n    librdkafka_options:\n      sasl.mechanism: ${SASL_MECHANISM}\n",
            "sources:\n  input:\n    type: kafka\n    librdkafka_options:\n      sasl.mechanisms: ${SASL_MECHANISMS}\n",
            "sources:\n  input:\n    type: http_client\n    query:\n      host:\n        type: ${QUERY_TYPE}\n        value: get_hostname!()\n",
        ] {
            let config = serde_yaml::from_str::<FeatureConfig>(config).expect("config must parse");
            let error = from_config(&config, &DECLARED_FEATURES)
                .expect_err("interpolated selector must fail");
            assert!(error.to_string().contains("use `cargo run` instead"));
        }

        assert_eq!(
            features(
                "sources:\n  input:\n    type: file\n    include: [app.log]\n    fingerprint:\n      strategy: ${FINGERPRINT_STRATEGY}\n",
            ),
            ["sources-file"]
        );
    }

    #[test]
    fn rejects_dynamic_vrl_programs() {
        for config in [
            "transforms:\n  remap:\n    type: remap\n    source: ${VRL_SOURCE}\n",
            "sources:\n  input:\n    type: http_client\n    query:\n      host:\n        type: vrl\n        value: SECRET[programs.query]\n",
            "transforms:\n  route:\n    type: route\n    route:\n      dynamic:\n        type: vrl\n        source: ${ROUTE_CONDITION}\n",
            "tests:\n  - condition: ${TEST_CONDITION}\n",
        ] {
            let config = serde_yaml::from_str::<FeatureConfig>(config).expect("config must parse");
            let error = from_config(&config, &DECLARED_FEATURES)
                .expect_err("dynamic VRL program must fail");
            assert!(error.to_string().contains("use `cargo run` instead"));
        }
    }

    #[test]
    fn allows_non_interpolation_syntax_in_vrl_programs() {
        for source in [r#".currency = "$""#, r#".message = "SECRET[missing_dot]""#] {
            let config =
                format!("transforms:\n  remap:\n    type: remap\n    source: '{source}'\n");

            assert!(features(&config).contains(&"transforms-remap".into()));
        }
    }

    #[test]
    fn generated_component_examples_only_use_declared_features() {
        let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("vdev must have a parent directory");
        let examples_root = repo_root.join("website/generated/example-configs");

        // Published vdev packages do not include Vector's generated examples.
        if !examples_root.exists() {
            return;
        }

        let mut missing = Vec::new();

        for kind in ["sources", "transforms", "sinks"] {
            let pattern = examples_root
                .join(kind)
                .join("*/*.yaml")
                .display()
                .to_string();
            for path in glob::glob(&pattern).expect("example glob must be valid") {
                let path = path.expect("example path must be readable");
                let config = fs::read_to_string(&path).expect("example config must be readable");
                for feature in features(&config) {
                    if !DECLARED_FEATURES.contains(&feature) {
                        missing.push(format!("{} -> {feature}", path.display()));
                    }
                }
            }
        }

        assert!(
            missing.is_empty(),
            "generated examples produced undeclared features:\n{}",
            missing.join("\n")
        );
    }
}
