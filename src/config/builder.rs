#[cfg(feature = "enterprise")]
use std::collections::BTreeMap;
use std::{path::Path, time::Duration};

use indexmap::IndexMap;
#[cfg(feature = "enterprise")]
use serde_json::Value;
use vector_lib::config::GlobalOptions;
use vector_lib::configurable::configurable_component;

use crate::{enrichment_tables::EnrichmentTables, providers::Providers, secrets::SecretBackends};

#[cfg(feature = "api")]
use super::api;
#[cfg(feature = "enterprise")]
use super::enterprise;
use super::{
    compiler, schema, BoxedSink, BoxedSource, BoxedTransform, ComponentKey, Config,
    EnrichmentTableOuter, HealthcheckOptions, SinkOuter, SourceOuter, TestDefinition,
    TransformOuter,
};

/// A complete Vector configuration.
#[configurable_component]
#[derive(Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct ConfigBuilder {
    #[serde(flatten)]
    pub global: GlobalOptions,

    #[cfg(feature = "api")]
    #[configurable(derived)]
    #[serde(default)]
    pub api: api::Options,

    #[configurable(derived)]
    #[configurable(metadata(docs::hidden))]
    #[serde(default)]
    pub schema: schema::Options,

    #[cfg(feature = "enterprise")]
    #[configurable(derived)]
    #[serde(default)]
    pub enterprise: Option<enterprise::Options>,

    #[configurable(derived)]
    #[serde(default)]
    pub healthchecks: HealthcheckOptions,

    /// All configured enrichment tables.
    #[serde(default)]
    pub enrichment_tables: IndexMap<ComponentKey, EnrichmentTableOuter>,

    /// All configured sources.
    #[serde(default)]
    pub sources: IndexMap<ComponentKey, SourceOuter>,

    /// All configured sinks.
    #[serde(default)]
    pub sinks: IndexMap<ComponentKey, SinkOuter<String>>,

    /// All configured transforms.
    #[serde(default)]
    pub transforms: IndexMap<ComponentKey, TransformOuter<String>>,

    /// All configured unit tests.
    #[serde(default)]
    pub tests: Vec<TestDefinition<String>>,

    /// Optional configuration provider to use.
    ///
    /// Configuration providers allow sourcing configuration information from a source other than
    /// the typical configuration files that must be passed to Vector.
    pub provider: Option<Providers>,

    /// All configured secrets backends.
    #[serde(default)]
    pub secret: IndexMap<ComponentKey, SecretBackends>,

    /// The duration in seconds to wait for graceful shutdown after SIGINT or SIGTERM are received.
    /// After the duration has passed, Vector will force shutdown. Default value is 60 seconds. This
    /// value can be set using a [cli arg](crate::cli::RootOpts::graceful_shutdown_limit_secs).
    #[serde(default, skip)]
    #[doc(hidden)]
    pub graceful_shutdown_duration: Option<Duration>,

    /// Allow the configuration to be empty, resulting in a topology with no components.
    #[serde(default, skip)]
    #[doc(hidden)]
    pub allow_empty: bool,
}

#[cfg(feature = "enterprise")]
#[derive(::serde::Serialize)]
struct ConfigBuilderHash<'a> {
    version: String,
    #[cfg(feature = "api")]
    api: &'a api::Options,
    schema: &'a schema::Options,
    global: &'a GlobalOptions,
    healthchecks: &'a HealthcheckOptions,
    enrichment_tables: BTreeMap<&'a ComponentKey, &'a EnrichmentTableOuter>,
    sources: BTreeMap<&'a ComponentKey, &'a SourceOuter>,
    sinks: BTreeMap<&'a ComponentKey, &'a SinkOuter<String>>,
    transforms: BTreeMap<&'a ComponentKey, &'a TransformOuter<String>>,
    tests: &'a Vec<TestDefinition<String>>,
    provider: &'a Option<Providers>,
    secret: BTreeMap<&'a ComponentKey, &'a SecretBackends>,
}

#[cfg(feature = "enterprise")]
impl ConfigBuilderHash<'_> {
    /// Sort inner JSON values to maintain a consistent ordering. This prevents
    /// non-deterministically serializable structures like HashMap from
    /// affecting the resulting hash. As a consequence, ordering that does not
    /// affect the actual semantics of a configuration is not considered when
    /// calculating the hash.
    fn into_hash(self) -> String {
        use sha2::{Digest, Sha256};

        let value = to_sorted_json_string(self);
        let output = Sha256::digest(value.as_bytes());

        hex::encode(output)
    }
}

/// It may seem like converting to Value prior to serializing to JSON string is
/// sufficient to sort our underlying keys. By default, Value::Map is backed by
/// BTreeMap which maintains an implicit key order, so it's an enticing and
/// simple approach. The issue however is the "by default". The underlying
/// Value::Map structure can actually change depending on which serde features
/// are enabled: IndexMap is the alternative and would break our intended
/// behavior.
///
/// Rather than rely on the opaque underlying serde structures, we are explicit
/// about sorting, sacrificing a bit of potential convenience for correctness.
#[cfg(feature = "enterprise")]
fn to_sorted_json_string<T>(value: T) -> String
where
    T: ::serde::Serialize,
{
    let mut value = serde_json::to_value(value).expect("Should serialize to JSON. Please report.");
    sort_json_value(&mut value);

    serde_json::to_string(&value).expect("Should serialize Value to JSON string. Please report.")
}

#[cfg(feature = "enterprise")]
fn sort_json_value(value: &mut Value) {
    match value {
        Value::Array(arr) => {
            for v in arr.iter_mut() {
                sort_json_value(v);
            }
        }
        Value::Object(map) => {
            let mut ordered_map: BTreeMap<String, Value> =
                serde_json::from_value(map.to_owned().into())
                    .expect("Converting Value to BTreeMap failed.");
            for v in ordered_map.values_mut() {
                sort_json_value(v);
            }
            *value = serde_json::to_value(ordered_map)
                .expect("Converting BTreeMap back to Value failed.");
        }
        _ => {}
    }
}

#[cfg(feature = "enterprise")]
impl<'a> From<&'a ConfigBuilder> for ConfigBuilderHash<'a> {
    fn from(value: &'a ConfigBuilder) -> Self {
        ConfigBuilderHash {
            version: crate::get_version(),
            #[cfg(feature = "api")]
            api: &value.api,
            schema: &value.schema,
            global: &value.global,
            healthchecks: &value.healthchecks,
            enrichment_tables: value.enrichment_tables.iter().collect(),
            sources: value.sources.iter().collect(),
            sinks: value.sinks.iter().collect(),
            transforms: value.transforms.iter().collect(),
            tests: &value.tests,
            provider: &value.provider,
            secret: value.secret.iter().collect(),
        }
    }
}

impl From<Config> for ConfigBuilder {
    fn from(config: Config) -> Self {
        let Config {
            global,
            #[cfg(feature = "api")]
            api,
            schema,
            #[cfg(feature = "enterprise")]
            enterprise,
            healthchecks,
            enrichment_tables,
            sources,
            sinks,
            transforms,
            tests,
            secret,
            graceful_shutdown_duration,
            hash: _,
        } = config;

        let transforms = transforms
            .into_iter()
            .map(|(key, transform)| (key, transform.map_inputs(ToString::to_string)))
            .collect();

        let sinks = sinks
            .into_iter()
            .map(|(key, sink)| (key, sink.map_inputs(ToString::to_string)))
            .collect();

        let tests = tests.into_iter().map(TestDefinition::stringify).collect();

        ConfigBuilder {
            global,
            #[cfg(feature = "api")]
            api,
            schema,
            #[cfg(feature = "enterprise")]
            enterprise,
            healthchecks,
            enrichment_tables,
            sources,
            sinks,
            transforms,
            provider: None,
            tests,
            secret,
            graceful_shutdown_duration,
            allow_empty: false,
        }
    }
}

impl ConfigBuilder {
    pub fn build(self) -> Result<Config, Vec<String>> {
        let (config, warnings) = self.build_with_warnings()?;

        for warning in warnings {
            warn!("{}", warning);
        }

        Ok(config)
    }

    pub fn build_with_warnings(self) -> Result<(Config, Vec<String>), Vec<String>> {
        compiler::compile(self)
    }

    pub fn add_enrichment_table<K: Into<String>, E: Into<EnrichmentTables>>(
        &mut self,
        key: K,
        enrichment_table: E,
    ) {
        self.enrichment_tables.insert(
            ComponentKey::from(key.into()),
            EnrichmentTableOuter::new(enrichment_table),
        );
    }

    pub fn add_source<K: Into<String>, S: Into<BoxedSource>>(&mut self, key: K, source: S) {
        self.sources
            .insert(ComponentKey::from(key.into()), SourceOuter::new(source));
    }

    pub fn add_sink<K: Into<String>, S: Into<BoxedSink>>(
        &mut self,
        key: K,
        inputs: &[&str],
        sink: S,
    ) {
        let inputs = inputs
            .iter()
            .map(|value| value.to_string())
            .collect::<Vec<_>>();
        let sink = SinkOuter::new(inputs, sink);
        self.add_sink_outer(key, sink);
    }

    pub fn add_sink_outer<K: Into<String>>(&mut self, key: K, sink: SinkOuter<String>) {
        self.sinks.insert(ComponentKey::from(key.into()), sink);
    }

    // For some feature sets, no transforms are compiled, which leads to no callers using this
    // method, and in turn, annoying errors about unused variables.
    pub fn add_transform(
        &mut self,
        key: impl Into<String>,
        inputs: &[&str],
        transform: impl Into<BoxedTransform>,
    ) {
        let inputs = inputs
            .iter()
            .map(|value| value.to_string())
            .collect::<Vec<_>>();
        let transform = TransformOuter::new(inputs, transform);

        self.transforms
            .insert(ComponentKey::from(key.into()), transform);
    }

    pub fn set_data_dir(&mut self, path: &Path) {
        self.global.data_dir = Some(path.to_owned());
    }

    pub fn append(&mut self, with: Self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        #[cfg(feature = "api")]
        if let Err(error) = self.api.merge(with.api) {
            errors.push(error);
        }

        #[cfg(feature = "enterprise")]
        {
            match (self.enterprise.as_ref(), with.enterprise) {
                (Some(_), Some(_)) => {
                    errors.push(
                        "duplicate 'enterprise' definition, only one definition allowed".to_owned(),
                    );
                }
                (None, Some(other)) => {
                    self.enterprise = Some(other);
                }
                _ => {}
            };
        }

        self.provider = with.provider;

        match self.global.merge(with.global) {
            Err(errs) => errors.extend(errs),
            Ok(new_global) => self.global = new_global,
        }

        self.schema.append(with.schema, &mut errors);

        self.schema.log_namespace = self.schema.log_namespace.or(with.schema.log_namespace);

        self.healthchecks.merge(with.healthchecks);

        with.enrichment_tables.keys().for_each(|k| {
            if self.enrichment_tables.contains_key(k) {
                errors.push(format!("duplicate enrichment_table name found: {}", k));
            }
        });
        with.sources.keys().for_each(|k| {
            if self.sources.contains_key(k) {
                errors.push(format!("duplicate source id found: {}", k));
            }
        });
        with.sinks.keys().for_each(|k| {
            if self.sinks.contains_key(k) {
                errors.push(format!("duplicate sink id found: {}", k));
            }
        });
        with.transforms.keys().for_each(|k| {
            if self.transforms.contains_key(k) {
                errors.push(format!("duplicate transform id found: {}", k));
            }
        });
        with.tests.iter().for_each(|wt| {
            if self.tests.iter().any(|t| t.name == wt.name) {
                errors.push(format!("duplicate test name found: {}", wt.name));
            }
        });
        with.secret.keys().for_each(|k| {
            if self.secret.contains_key(k) {
                errors.push(format!("duplicate secret id found: {}", k));
            }
        });
        if !errors.is_empty() {
            return Err(errors);
        }

        self.enrichment_tables.extend(with.enrichment_tables);
        self.sources.extend(with.sources);
        self.sinks.extend(with.sinks);
        self.transforms.extend(with.transforms);
        self.tests.extend(with.tests);
        self.secret.extend(with.secret);

        Ok(())
    }

    #[cfg(feature = "enterprise")]
    /// SHA256 hexadecimal representation of a config builder. This is generated by serializing
    /// an order-stable JSON of the config builder and feeding its bytes into a SHA256 hasher.
    pub fn sha256_hash(&self) -> String {
        ConfigBuilderHash::from(self).into_hash()
    }

    #[cfg(test)]
    pub fn from_toml(input: &str) -> Self {
        crate::config::format::deserialize(input, crate::config::format::Format::Toml).unwrap()
    }

    #[cfg(test)]
    pub fn from_json(input: &str) -> Self {
        crate::config::format::deserialize(input, crate::config::format::Format::Json).unwrap()
    }
}

#[cfg(all(
    test,
    feature = "enterprise",
    feature = "api",
    feature = "sources-demo_logs",
    feature = "sinks-loki"
))]
mod tests {
    use indexmap::IndexMap;

    use crate::config::{
        builder::{sort_json_value, to_sorted_json_string},
        enterprise, ConfigBuilder,
    };

    use super::ConfigBuilderHash;

    #[test]
    /// If this test fails, it likely means an implementation detail has changed
    /// which is likely to impact the final hash.
    fn version_json_order() {
        use serde_json::{json, Value};

        use super::{ConfigBuilder, ConfigBuilderHash};

        // Expected key order. This is important for guaranteeing that a hash is
        // reproducible across versions.
        let expected_keys = [
            "api",
            "enrichment_tables",
            "global",
            "healthchecks",
            "provider",
            "schema",
            "secret",
            "sinks",
            "sources",
            "tests",
            "transforms",
            "version",
        ];

        let builder = ConfigBuilder::default();

        let mut value = json!(ConfigBuilderHash::from(&builder));
        sort_json_value(&mut value);

        match value {
            // Should serialize to a map.
            Value::Object(map) => {
                // Check ordering.
                assert!(map.keys().eq(expected_keys));
            }
            _ => panic!("should serialize to object"),
        }
    }

    #[test]
    /// If this hash changes, it means either the version of Vector has changed (here it's fixed),
    /// the `ConfigBuilder` has changed what it serializes, or the implementation of `serde_json` has changed.
    /// If this test fails, we should ideally be able to fix so that the original hash passes!
    fn version_hash_match() {
        let expected_hash = "6c98bea9d9e2f3133e2d39ba04592d17f96340a9bc4c8d697b09f5af388a76bd";
        let builder = ConfigBuilder::default();
        let mut hash_builder = ConfigBuilderHash::from(&builder);
        hash_builder.version = "1.2.3".into();
        assert_eq!(expected_hash, hash_builder.into_hash());
    }

    #[test]
    fn append_keeps_enterprise() {
        let mut base = ConfigBuilder {
            enterprise: Some(enterprise::Options::default()),
            ..Default::default()
        };
        let other = ConfigBuilder::default();
        base.append(other).unwrap();
        assert!(base.enterprise.is_some());
    }

    #[test]
    fn append_sets_enterprise() {
        let mut base = ConfigBuilder::default();
        let other = ConfigBuilder {
            enterprise: Some(enterprise::Options::default()),
            ..Default::default()
        };
        base.append(other).unwrap();
        assert!(base.enterprise.is_some());
    }

    #[test]
    fn append_overwrites_enterprise() {
        let base_ent = enterprise::Options::default();
        let mut base = ConfigBuilder {
            enterprise: Some(base_ent),
            ..Default::default()
        };
        let other_ent = enterprise::Options::default();
        let other = ConfigBuilder {
            enterprise: Some(other_ent),
            ..Default::default()
        };
        let errors = base.append(other).unwrap_err();
        assert_eq!(
            errors[0],
            "duplicate 'enterprise' definition, only one definition allowed"
        );
    }

    #[test]
    fn version_hash_sorted() {
        let control_config = toml::from_str::<ConfigBuilder>(
            r#"
        [enterprise]
        api_key = "apikey"
        configuration_key = "configkey"

        [sources.foo]
        type = "internal_logs"

        [sinks.loki]
        type = "loki"
        endpoint = "https://localhost:1111"
        inputs = ["foo"]

        [sinks.loki.labels]
        foo = '{{ foo }}'
        bar = '{{ bar }}'
        baz = '{{ baz }}'
        ingest = "hello-world"
        level = '{{ level }}'
        module = '{{ module }}'
        service = '{{ service }}'

        [sinks.loki.encoding]
        codec = "json"
        "#,
        )
        .unwrap();
        let expected_hash = ConfigBuilderHash::from(&control_config).into_hash();
        for _ in 0..100 {
            let experiment_config = toml::from_str::<ConfigBuilder>(
                r#"
            [enterprise]
            api_key = "apikey"
            configuration_key = "configkey"

            [sources.foo]
            type = "internal_logs"

            [sinks.loki]
            type = "loki"
            endpoint = "https://localhost:1111"
            inputs = ["foo"]

            [sinks.loki.labels]
            foo = '{{ foo }}'
            bar = '{{ bar }}'
            baz = '{{ baz }}'
            ingest = "hello-world"
            level = '{{ level }}'
            module = '{{ module }}'
            service = '{{ service }}'

            [sinks.loki.encoding]
            codec = "json"
            "#,
            )
            .unwrap();
            assert_eq!(
                expected_hash,
                ConfigBuilderHash::from(&experiment_config).into_hash()
            );
        }
    }

    #[test]
    fn test_to_sorted_json_string() {
        let ordered_map = IndexMap::from([("z", 26), ("a", 1), ("d", 4), ("c", 3), ("b", 2)]);
        assert_eq!(
            r#"{"a":1,"b":2,"c":3,"d":4,"z":26}"#.to_string(),
            to_sorted_json_string(ordered_map)
        );
    }
}
