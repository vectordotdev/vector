#[cfg(feature = "enterprise")]
use std::collections::BTreeMap;
use std::path::Path;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
#[cfg(feature = "enterprise")]
use serde_json::Value;
use vector_core::{config::GlobalOptions, default_data_dir, transform::TransformConfig};

#[cfg(feature = "api")]
use super::api;
#[cfg(feature = "enterprise")]
use super::enterprise;
use super::{
    compiler, provider, schema, ComponentKey, Config, EnrichmentTableConfig, EnrichmentTableOuter,
    HealthcheckOptions, SecretBackend, SinkConfig, SinkOuter, SourceConfig, SourceOuter,
    TestDefinition, TransformOuter,
};

#[derive(Deserialize, Serialize, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct ConfigBuilder {
    #[serde(flatten)]
    pub global: GlobalOptions,
    #[cfg(feature = "api")]
    #[serde(default)]
    pub api: api::Options,
    #[serde(default)]
    pub schema: schema::Options,
    #[cfg(feature = "enterprise")]
    #[serde(default)]
    pub enterprise: Option<enterprise::Options>,
    #[serde(default)]
    pub healthchecks: HealthcheckOptions,
    #[serde(default)]
    pub enrichment_tables: IndexMap<ComponentKey, EnrichmentTableOuter>,
    #[serde(default)]
    pub sources: IndexMap<ComponentKey, SourceOuter>,
    #[serde(default)]
    pub sinks: IndexMap<ComponentKey, SinkOuter<String>>,
    #[serde(default)]
    pub transforms: IndexMap<ComponentKey, TransformOuter<String>>,
    #[serde(default)]
    pub tests: Vec<TestDefinition<String>>,
    pub provider: Option<Box<dyn provider::ProviderConfig>>,
    #[serde(default)]
    pub secret: IndexMap<ComponentKey, Box<dyn SecretBackend>>,
}

#[cfg(feature = "enterprise")]
#[derive(Serialize)]
struct ConfigBuilderHash<'a> {
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
    provider: &'a Option<Box<dyn provider::ProviderConfig>>,
    secret: BTreeMap<&'a ComponentKey, &'a dyn SecretBackend>,
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
    T: Serialize,
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
            secret: value.secret.iter().map(|(k, v)| (k, v.as_ref())).collect(),
        }
    }
}

impl Clone for ConfigBuilder {
    fn clone(&self) -> Self {
        // This is a hack around the issue of cloning
        // trait objects. So instead to clone the config
        // we first serialize it into JSON, then back from
        // JSON. Originally we used TOML here but TOML does not
        // support serializing `None`.
        let json = serde_json::to_value(self).unwrap();
        serde_json::from_value(json).unwrap()
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
            ..
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

    pub fn add_enrichment_table<E: EnrichmentTableConfig + 'static, T: Into<String>>(
        &mut self,
        name: T,
        enrichment_table: E,
    ) {
        self.enrichment_tables.insert(
            ComponentKey::from(name.into()),
            EnrichmentTableOuter::new(Box::new(enrichment_table)),
        );
    }

    pub fn add_source<S: SourceConfig + 'static, T: Into<String>>(&mut self, id: T, source: S) {
        self.sources
            .insert(ComponentKey::from(id.into()), SourceOuter::new(source));
    }

    pub fn add_sink<S: SinkConfig + 'static, T: Into<String>>(
        &mut self,
        id: T,
        inputs: &[&str],
        sink: S,
    ) {
        let inputs = inputs
            .iter()
            .map(|value| value.to_string())
            .collect::<Vec<_>>();
        let sink = SinkOuter::new(inputs, Box::new(sink));
        self.add_sink_outer(id, sink);
    }

    pub fn add_sink_outer(&mut self, id: impl Into<String>, sink: SinkOuter<String>) {
        self.sinks.insert(ComponentKey::from(id.into()), sink);
    }

    pub fn add_transform<T: TransformConfig + 'static, S: Into<String>>(
        &mut self,
        id: S,
        inputs: &[&str],
        transform: T,
    ) {
        let inputs = inputs
            .iter()
            .map(|value| value.to_string())
            .collect::<Vec<_>>();
        let transform = TransformOuter {
            inner: Box::new(transform),
            inputs,
        };

        self.transforms
            .insert(ComponentKey::from(id.into()), transform);
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

        self.schema = with.schema;

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

        if self.global.proxy.http.is_some() && with.global.proxy.http.is_some() {
            errors.push("conflicting values for 'proxy.http' found".to_owned());
        }

        if self.global.proxy.https.is_some() && with.global.proxy.https.is_some() {
            errors.push("conflicting values for 'proxy.https' found".to_owned());
        }

        if !self.global.proxy.no_proxy.is_empty() && !with.global.proxy.no_proxy.is_empty() {
            errors.push("conflicting values for 'proxy.no_proxy' found".to_owned());
        }

        self.global.proxy = self.global.proxy.merge(&with.global.proxy);

        if self.schema.log_namespace.is_some()
            && with.schema.log_namespace.is_some()
            && self.schema.log_namespace != with.schema.log_namespace
        {
            errors.push(
                format!("conflicting values for 'log_namespace' found. Both {:?} and {:?} used in the same component",
                                self.schema.log_namespace(), with.schema.log_namespace())
            );
        }

        self.schema.log_namespace = self.schema.log_namespace.or(with.schema.log_namespace);

        if self.global.data_dir.is_none() || self.global.data_dir == default_data_dir() {
            self.global.data_dir = with.global.data_dir;
        } else if with.global.data_dir != default_data_dir()
            && self.global.data_dir != with.global.data_dir
        {
            // If two configs both set 'data_dir' and have conflicting values
            // we consider this an error.
            errors.push("conflicting values for 'data_dir' found".to_owned());
        }

        // If the user has multiple config files, we must *merge* log schemas
        // until we meet a conflict, then we are allowed to error.
        if let Err(merge_errors) = self.global.log_schema.merge(&with.global.log_schema) {
            errors.extend(merge_errors);
        }

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
    /// If this hash changes, it means either the `ConfigBuilder` has changed what it
    /// serializes, or the implementation of `serde_json` has changed. If this test fails, we
    /// should ideally be able to fix so that the original hash passes!
    fn version_hash_match() {
        assert_eq!(
            "53dff3cdc4bcf9ac23a04746b253b2f3ba8b1120e483e13d586b3643a4e066de",
            ConfigBuilder::default().sha256_hash()
        );
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
        let mut base_ent = enterprise::Options::default();
        base_ent.application_key = "base".to_string();
        let mut base = ConfigBuilder {
            enterprise: Some(base_ent),
            ..Default::default()
        };
        let mut other_ent = enterprise::Options::default();
        other_ent.application_key = "other".to_string();
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
        application_key = "appkey"
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
            application_key = "appkey"
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
