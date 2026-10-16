use std::{
    collections::{BTreeSet, HashMap},
    ffi::OsStr,
    fs,
    path::Path,
    process::Command,
};

use anyhow::{Context, Result, bail};
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
const VRL_FEATURES: &[&str] = &[
    "vector-vrl-functions/dnstap",
    "vrl-functions-crypto",
    "vrl-functions-env",
    "vrl-functions-network",
    "vrl-functions-system",
];
const VRL_DISCRIMINATORS: &[(&str, &str)] = &[("type", "vrl"), ("codec", "vrl")];
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

const UNSUPPORTED_DYNAMIC_FEATURE_SELECTOR: &str = "cargo vdev feature selection does not support environment or secret interpolation in feature selectors; use `cargo run` instead";
const UNSUPPORTED_YAML_MERGE_KEY: &str =
    "cargo vdev feature selection does not support YAML merge keys; use `cargo run` instead";
const UNSUPPORTED_PROVIDER: &str = "cargo vdev feature selection does not support configuration providers; use `cargo run` instead";

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
        Some("yaml" | "yml") => {
            let value: serde_yaml::Value = serde_yaml::from_str(&config)?;
            if contains_yaml_merge_key(&value) {
                bail!(UNSUPPORTED_YAML_MERGE_KEY);
            }
            serde_yaml::from_value(value)?
        }
        Some(_) => bail!("Invalid filename {}, unknown extension", filename.display()),
    };

    let declared_features = CargoToml::load_from(&find_repo_root()?.join("Cargo.toml"))?
        .features
        .into_keys()
        .collect();

    from_config(&config, &declared_features)
}

fn contains_yaml_merge_key(value: &serde_yaml::Value) -> bool {
    match value {
        serde_yaml::Value::Sequence(values) => values.iter().any(contains_yaml_merge_key),
        serde_yaml::Value::Mapping(mapping) => mapping
            .iter()
            .any(|(key, value)| key.as_str() == Some("<<") || contains_yaml_merge_key(value)),
        serde_yaml::Value::Tagged(tagged) => contains_yaml_merge_key(&tagged.value),
        _ => false,
    }
}

fn from_config(config: &FeatureConfig, declared_features: &FeatureSet) -> Result<Vec<String>> {
    if config.provider.is_some() {
        bail!(UNSUPPORTED_PROVIDER);
    }
    if [
        &config.enrichment_tables,
        &config.secret,
        &config.sources,
        &config.transforms,
        &config.sinks,
    ]
    .into_iter()
    .flat_map(|section| section.values())
    .any(|component| has_dynamic_reference(&component.r#type))
    {
        bail!(UNSUPPORTED_DYNAMIC_FEATURE_SELECTOR);
    }
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
                get_nested_features(&mut features, &mut uses_vrl, Some(key.as_str()), value)?;
            }
        }
    }
    for (key, value) in &config.other {
        get_nested_features(&mut features, &mut uses_vrl, Some(key.as_str()), value)?;
    }

    if uses_vrl {
        features.extend(VRL_FEATURES.iter().map(|feature| (*feature).into()));
    }

    Ok(features.into_iter().collect())
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
            .filter_map(|component| component_feature(key, &component.r#type, declared_features)),
    );
}

fn component_feature(
    key: &str,
    component_type: &str,
    declared_features: &FeatureSet,
) -> Option<String> {
    let mut prefix = component_type;

    loop {
        let candidate = format!("{key}-{prefix}");
        if declared_features.contains(&candidate) {
            return Some(candidate);
        }
        let (shorter, _) = prefix.rsplit_once('_')?;
        prefix = shorter;
    }
}

fn get_nested_features(
    features: &mut FeatureSet,
    uses_vrl: &mut bool,
    parent_key: Option<&str>,
    value: &Value,
) -> Result<()> {
    match value {
        Value::Array(values) => {
            for value in values {
                get_nested_features(features, uses_vrl, parent_key, value)?;
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
                if is_feature_selector(parent_key, key)
                    && value.as_str().is_some_and(has_dynamic_reference)
                {
                    bail!(UNSUPPORTED_DYNAMIC_FEATURE_SELECTOR);
                }
                get_nested_features(features, uses_vrl, Some(key.as_str()), value)?;
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }

    Ok(())
}

fn is_feature_selector(parent_key: Option<&str>, key: &str) -> bool {
    NESTED_FEATURE_RULES.iter().any(|rule| {
        rule.key == key
            && rule
                .parent_key
                .is_none_or(|required| parent_key == Some(required))
    }) || VRL_DISCRIMINATORS
        .iter()
        .any(|(selector, _)| *selector == key)
        || (parent_key == Some(KAFKA_OPTIONS_KEY) && KAFKA_GSSAPI_KEYS.contains(&key))
}

fn has_dynamic_reference(value: &str) -> bool {
    value.contains('$') || value.contains("SECRET[")
}

fn kafka_gssapi_feature() -> &'static str {
    if cfg!(target_os = "linux") && !system_sasl_available() {
        GSSAPI_VENDORED_FEATURE
    } else {
        GSSAPI_FEATURE
    }
}

fn system_sasl_available() -> bool {
    Command::new("pkg-config")
        .args(["--exists", "libsasl2"])
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(test)]
mod tests {
    use indoc::indoc;
    use std::{fs, path::Path, sync::LazyLock};

    use super::{
        CargoToml, FeatureConfig, FeatureSet, from_config, kafka_gssapi_feature, load_and_extract,
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
        let config = indoc! {"
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
        "};

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
                "vector-vrl-functions/dnstap",
                "vrl-functions-crypto",
                "vrl-functions-env",
                "vrl-functions-network",
                "vrl-functions-system",
            ]
        );
    }

    #[test]
    fn extracts_top_level_feature_gates() {
        let config = indoc! {"
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
        "};

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
            features(indoc! {"
                transforms:
                  metrics:
                    type: log_to_metric
            "}),
            [
                "transforms-log_to_metric",
                "vector-vrl-functions/dnstap",
                "vrl-functions-crypto",
                "vrl-functions-env",
                "vrl-functions-network",
                "vrl-functions-system",
            ]
        );
    }

    #[test]
    fn extracts_nested_codec_and_enables_all_vrl_function_features() {
        let config = indoc! {r#"
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
        "#};

        assert_eq!(
            features(config),
            [
                "codecs-parquet",
                "sinks-aws_s3",
                "transforms-remap",
                "vector-vrl-functions/dnstap",
                "vrl-functions-crypto",
                "vrl-functions-env",
                "vrl-functions-network",
                "vrl-functions-system",
            ]
        );
    }

    #[test]
    fn extracts_all_gated_codecs_and_aws_auth() {
        let config = indoc! {"
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
        "};

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
                indoc! {"
                    sources:
                      kafka:
                        type: kafka
                        librdkafka_options:
                          {key}: GSSAPI
                "},
                key = key,
            );
            assert_eq!(
                features(&config),
                [
                    kafka_gssapi_feature().to_owned(),
                    "sources-kafka".to_owned(),
                ]
            );
        }
    }

    #[test]
    fn enables_all_vrl_function_features_for_any_transform() {
        let config = indoc! {"
            transforms:
              transform:
                type: dedupe
        "};
        let features = features(config);

        for feature in [
            "vector-vrl-functions/dnstap",
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
            indoc! {"
                sources:
                  input:
                    type: http_client
                    query:
                      host:
                        type: vrl
                        value: get_hostname!()
            "},
            indoc! {"
                sources:
                  input:
                    type: stdin
                    decoding:
                      codec: vrl
                      vrl:
                        source: get_hostname!()
            "},
            indoc! {r#"
                sources:
                  input:
                    type: http_server
                    auth:
                      strategy: custom
                      source: get_hostname!() == "host"
            "#},
            indoc! {r#"
                tests:
                  - condition: .message == "hello"
            "#},
        ] {
            let features = features(config);

            for feature in [
                "vector-vrl-functions/dnstap",
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
        let config = serde_yaml::from_str::<FeatureConfig>(indoc! {"
            provider:
              type: http
              url: https://example.com/vector.yaml
        "})
        .expect("config must parse");
        let error =
            from_config(&config, &DECLARED_FEATURES).expect_err("configuration provider must fail");

        assert!(error.to_string().contains("use `cargo run` instead"));
    }

    #[test]
    fn rejects_dynamic_feature_selectors() {
        for config in [
            indoc! {"
                    sources:
                      input:
                        type: '${SOURCE_TYPE}'
                "},
            indoc! {"
                    sources:
                      input:
                        type: 'SECRET[features.source_type]'
                "},
            indoc! {"
                    sources:
                      input:
                        type: socket
                        decoding:
                          codec: '${CODEC}'
                "},
            indoc! {"
                    sources:
                      input:
                        type: socket
                        decoding:
                          codec: 'SECRET[features.codec]'
                "},
        ] {
            let config = serde_yaml::from_str::<FeatureConfig>(config).expect("config must parse");
            let error = from_config(&config, &DECLARED_FEATURES)
                .expect_err("dynamic feature selector must fail");

            assert!(error.to_string().contains("feature selectors"));
            assert!(error.to_string().contains("use `cargo run` instead"));
        }
    }

    #[test]
    fn rejects_yaml_merge_keys() {
        let config = tempfile::Builder::new()
            .suffix(".yaml")
            .tempfile()
            .expect("temporary config must be created");
        fs::write(
            config.path(),
            indoc! {"
                defaults: &defaults
                  type: console
                sinks:
                  output:
                    <<: *defaults
            "},
        )
        .expect("temporary config must be written");

        let error = load_and_extract(config.path()).expect_err("YAML merge key must fail");

        assert!(error.to_string().contains("YAML merge keys"));
        assert!(error.to_string().contains("use `cargo run` instead"));
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
                    // Cargo also accepts dependency features as `dependency/feature`.
                    if !feature.contains('/') && !DECLARED_FEATURES.contains(&feature) {
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
