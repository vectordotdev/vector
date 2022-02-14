#[cfg(feature = "datadog-pipelines")]
use std::collections::BTreeMap;
use std::path::Path;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use vector_core::{config::GlobalOptions, default_data_dir, transform::TransformConfig};

#[cfg(feature = "api")]
use super::api;
#[cfg(feature = "datadog-pipelines")]
use super::datadog;
use super::{
    compiler, provider, ComponentKey, Config, EnrichmentTableConfig, EnrichmentTableOuter,
    HealthcheckOptions, SinkConfig, SinkOuter, SourceConfig, SourceOuter, TestDefinition,
    TransformOuter,
};

#[derive(Deserialize, Serialize, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct ConfigBuilder {
    #[serde(flatten)]
    pub global: GlobalOptions,
    #[cfg(feature = "api")]
    #[serde(default)]
    pub api: api::Options,
    #[cfg(feature = "datadog-pipelines")]
    #[serde(default)]
    pub datadog: Option<datadog::Options>,
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
}

#[cfg(feature = "datadog-pipelines")]
#[derive(Serialize)]
struct ConfigBuilderHash<'a> {
    #[cfg(feature = "api")]
    api: &'a api::Options,
    global: &'a GlobalOptions,
    healthchecks: &'a HealthcheckOptions,
    enrichment_tables: BTreeMap<&'a ComponentKey, &'a EnrichmentTableOuter>,
    sources: BTreeMap<&'a ComponentKey, &'a SourceOuter>,
    sinks: BTreeMap<&'a ComponentKey, &'a SinkOuter<String>>,
    transforms: BTreeMap<&'a ComponentKey, &'a TransformOuter<String>>,
    tests: &'a Vec<TestDefinition<String>>,
    provider: &'a Option<Box<dyn provider::ProviderConfig>>,
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
            #[cfg(feature = "datadog-pipelines")]
            datadog,
            healthchecks,
            enrichment_tables,
            sources,
            sinks,
            transforms,
            tests,
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
            #[cfg(feature = "datadog-pipelines")]
            datadog,
            healthchecks,
            enrichment_tables,
            sources,
            sinks,
            transforms,
            provider: None,
            tests,
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

        #[cfg(feature = "datadog-pipelines")]
        {
            self.datadog = with.datadog;
            if let Some(datadog) = &self.datadog {
                if datadog.enabled {
                    // enable other enterprise features
                    self.global.enterprise = true;
                }
            }
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
        if !errors.is_empty() {
            return Err(errors);
        }

        self.enrichment_tables.extend(with.enrichment_tables);
        self.sources.extend(with.sources);
        self.sinks.extend(with.sinks);
        self.transforms.extend(with.transforms);
        self.tests.extend(with.tests);

        Ok(())
    }

    #[cfg(feature = "datadog-pipelines")]
    /// SHA256 hexadecimal representation of a config builder. This is generated by serializing
    /// an order-stable JSON of the config builder and feeding its bytes into a SHA256 hasher.
    pub fn sha256_hash(&self) -> String {
        use sha2::{Digest, Sha256};

        let value = serde_json::to_string(&ConfigBuilderHash {
            #[cfg(feature = "api")]
            api: &self.api,
            global: &self.global,
            healthchecks: &self.healthchecks,
            enrichment_tables: self.enrichment_tables.iter().collect(),
            sources: self.sources.iter().collect(),
            sinks: self.sinks.iter().collect(),
            transforms: self.transforms.iter().collect(),
            tests: &self.tests,
            provider: &self.provider,
        })
        .expect("should serialize to JSON");

        let output = Sha256::digest(value.as_bytes());

        hex::encode(output)
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

#[cfg(all(test, feature = "datadog-pipelines", feature = "api"))]
mod tests {
    use crate::config::ConfigBuilder;

    #[test]
    /// We are relying on `serde_json` to serialize keys in the ordered provided. If this test
    /// fails, it likely means an implementation detail of serialization has changed, which is
    /// likely to impact the final hash.
    fn version_json_order() {
        use serde_json::{json, Value};

        use super::{ConfigBuilder, ConfigBuilderHash};

        // Expected key order of serialization. This is important for guaranteeing that a
        // hash is reproducible across versions.
        let expected_keys = [
            "api",
            "global",
            "healthchecks",
            "enrichment_tables",
            "sources",
            "sinks",
            "transforms",
            "tests",
            "provider",
        ];

        let builder = ConfigBuilder::default();

        let value = json!(ConfigBuilderHash {
            api: &builder.api,
            global: &builder.global,
            healthchecks: &builder.healthchecks,
            enrichment_tables: builder.enrichment_tables.iter().collect(),
            sources: builder.sources.iter().collect(),
            sinks: builder.sinks.iter().collect(),
            transforms: builder.transforms.iter().collect(),
            tests: &builder.tests,
            provider: &builder.provider,
        });

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
            "14def8ff43fe0255b3234a7c3d7488379a119b7dbcf311c77ad308a83173d92c",
            ConfigBuilder::default().sha256_hash()
        );
    }
}
