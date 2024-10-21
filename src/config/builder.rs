use std::{path::Path, time::Duration};

use indexmap::IndexMap;
use vector_lib::config::GlobalOptions;
use vector_lib::configurable::configurable_component;

use crate::{enrichment_tables::EnrichmentTables, providers::Providers, secrets::SecretBackends};

#[cfg(feature = "api")]
use super::api;
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

impl From<Config> for ConfigBuilder {
    fn from(config: Config) -> Self {
        let Config {
            global,
            #[cfg(feature = "api")]
            api,
            schema,
            healthchecks,
            enrichment_tables,
            sources,
            sinks,
            transforms,
            tests,
            secret,
            graceful_shutdown_duration,
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

    #[cfg(test)]
    pub fn from_toml(input: &str) -> Self {
        crate::config::format::deserialize(input, crate::config::format::Format::Toml).unwrap()
    }

    #[cfg(test)]
    pub fn from_json(input: &str) -> Self {
        crate::config::format::deserialize(input, crate::config::format::Format::Json).unwrap()
    }
}
