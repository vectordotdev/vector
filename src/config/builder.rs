#[cfg(feature = "api")]
use super::api;
#[cfg(feature = "datadog-pipelines")]
use super::datadog;
use super::{
    compiler, provider, ComponentKey, Config, EnrichmentTableConfig, EnrichmentTableOuter,
    HealthcheckOptions, SinkConfig, SinkOuter, SourceConfig, SourceOuter, TestDefinition,
    TransformOuter,
};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use vector_core::{config::GlobalOptions, default_data_dir, transform::TransformConfig};

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
    pub datadog: datadog::Options,
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
    pub tests: Vec<TestDefinition>,
    pub provider: Option<Box<dyn provider::ProviderConfig>>,
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
            expansions: _,
        } = config;

        let transforms = transforms
            .into_iter()
            .map(|(key, transform)| (key, transform.map_inputs(ToString::to_string)))
            .collect();

        let sinks = sinks
            .into_iter()
            .map(|(key, sink)| (key, sink.map_inputs(ToString::to_string)))
            .collect();

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
            tests: c.tests,
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

    pub fn append(&mut self, with: Self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        #[cfg(feature = "api")]
        if let Err(error) = self.api.merge(with.api) {
            errors.push(error);
        }

        #[cfg(feature = "datadog-pipelines")]
        {
            self.datadog = with.datadog;
            if self.datadog.enabled {
                // enable other enterprise features
                self.global.enterprise = true;
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

    #[cfg(test)]
    pub fn from_toml(input: &str) -> Self {
        crate::config::format::deserialize(input, Some(crate::config::format::Format::Toml))
            .unwrap()
    }

    #[cfg(test)]
    pub fn from_json(input: &str) -> Self {
        crate::config::format::deserialize(input, Some(crate::config::format::Format::Json))
            .unwrap()
    }
}
