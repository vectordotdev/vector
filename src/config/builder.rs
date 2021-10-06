#[cfg(feature = "api")]
use super::api;
#[cfg(feature = "datadog-pipelines")]
use super::datadog;
use super::{
    compiler, pipeline::Pipelines, provider, ComponentKey, Config, EnrichmentTableConfig,
    EnrichmentTableOuter, HealthcheckOptions, SinkConfig, SinkOuter, SourceConfig, SourceOuter,
    TestDefinition, TransformOuter,
};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
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
    pub sinks: IndexMap<ComponentKey, SinkOuter>,
    #[serde(default)]
    pub transforms: IndexMap<ComponentKey, TransformOuter>,
    #[serde(default)]
    pub tests: Vec<TestDefinition>,
    pub provider: Option<Box<dyn provider::ProviderConfig>>,
    #[serde(default)]
    pub pipelines: Pipelines,
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
    fn from(c: Config) -> Self {
        ConfigBuilder {
            global: c.global,
            #[cfg(feature = "api")]
            api: c.api,
            #[cfg(feature = "datadog-pipelines")]
            datadog: c.datadog,
            healthchecks: c.healthchecks,
            enrichment_tables: c.enrichment_tables,
            sources: c.sources,
            sinks: c.sinks,
            transforms: c.transforms,
            provider: None,
            tests: c.tests,
            pipelines: Default::default(),
        }
    }
}

impl ConfigBuilder {
    // moves the pipeline transforms into regular scoped transforms
    // and add the output to the sources
    pub fn merge_pipelines(&mut self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();
        let global_inputs = self
            .transforms
            .keys()
            .chain(self.sources.keys())
            .filter(|id| id.is_global())
            .map(|id| id.id().to_string())
            .collect::<HashSet<_>>();

        let pipelines = std::mem::take(&mut self.pipelines);
        let pipeline_transforms = pipelines.into_scoped_transforms();

        for (component_id, pipeline_transform) in pipeline_transforms {
            // to avoid ambiguity, we forbid to use a component name in the pipeline scope
            // that is already used as the global scope.
            if global_inputs.contains(component_id.id()) {
                errors.push(format!(
                    "Component ID '{}' is already used.",
                    component_id.id()
                ));
                continue;
            }
            for input in pipeline_transform.outputs.iter() {
                if let Some(transform) = self.transforms.get_mut(input) {
                    transform.inputs.push(component_id.clone());
                } else if let Some(sink) = self.sinks.get_mut(input) {
                    sink.inputs.push(component_id.clone());
                } else {
                    errors.push(format!("Couldn't find transform or sink '{}'", input));
                }
            }
            self.transforms
                .insert(component_id, pipeline_transform.inner);
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

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
        let inputs = inputs.iter().map(ComponentKey::from).collect::<Vec<_>>();
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
            .map(|value| ComponentKey::from(value.to_string()))
            .collect::<Vec<_>>();
        let transform = TransformOuter {
            inner: Box::new(transform),
            inputs,
        };

        self.transforms
            .insert(ComponentKey::from(id.into()), transform);
    }

    pub fn set_pipelines(&mut self, pipelines: Pipelines) {
        self.pipelines = pipelines;
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

#[cfg(test)]
mod tests {
    use crate::config::pipeline::{Pipeline, Pipelines};
    use crate::config::ConfigBuilder;
    use indexmap::IndexMap;

    #[test]
    fn success() {
        let mut pipelines = IndexMap::new();
        pipelines.insert(
            "foo".into(),
            Pipeline::from_toml(
                r#"
        [transforms.bar]
        inputs = ["logs"]
        type = "remap"
        source = ""
        outputs = ["print"]
        "#,
            ),
        );
        let pipelines = Pipelines::from(pipelines);
        let mut builder = ConfigBuilder::from_toml(
            r#"
        [sources.logs]
        type = "generator"
        format = "syslog"

        [sinks.print]
        type = "console"
        encoding.codec = "json"
        "#,
        );
        builder.set_pipelines(pipelines);
        let result = builder.build();
        assert!(result.is_ok());
    }

    #[test]
    fn overlaping_transform_id() {
        let mut pipelines = IndexMap::new();
        pipelines.insert(
            "foo".into(),
            Pipeline::from_toml(
                r#"
        [transforms.bar]
        inputs = ["logs"]
        type = "remap"
        source = ""
        outputs = ["print"]
        "#,
            ),
        );
        let pipelines = Pipelines::from(pipelines);
        let mut builder = ConfigBuilder::from_toml(
            r#"
        [sources.logs]
        type = "generator"
        format = "syslog"

        [transforms.bar]
        inputs = ["logs"]
        type = "remap"
        source = ""

        [sinks.print]
        inputs = ["bar"]
        type = "console"
        encoding.codec = "json"
        "#,
        );
        builder.set_pipelines(pipelines);
        let errors = builder.build().unwrap_err();
        assert_eq!(errors[0], "Component ID 'bar' is already used.");
    }

    #[test]
    fn overlaping_pipeline_transform_id() {
        let mut pipelines = IndexMap::new();
        pipelines.insert(
            "foo".into(),
            Pipeline::from_toml(
                r#"
        [transforms.remap]
        inputs = ["logs"]
        type = "remap"
        source = ""
        outputs = ["print"]
        "#,
            ),
        );
        pipelines.insert(
            "bar".into(),
            Pipeline::from_toml(
                r#"
        [transforms.remap]
        inputs = ["logs"]
        type = "remap"
        source = ""
        outputs = ["print"]
        "#,
            ),
        );
        let pipelines = Pipelines::from(pipelines);
        let mut builder = ConfigBuilder::from_toml(
            r#"
        [sources.logs]
        type = "generator"
        format = "syslog"

        [sinks.print]
        type = "console"
        encoding.codec = "json"
        "#,
        );
        builder.set_pipelines(pipelines);
        let config = builder.build().unwrap();
        assert_eq!(config.transforms.len(), 2);
    }

    #[test]
    fn component_with_dot() {
        let builder = ConfigBuilder::from_json(
            r#"{
            "sources": {
                "log.with.dots": {
                    "type": "generator",
                    "format": "syslog"
                }
            },
            "sinks": {
                "print": {
                    "inputs": ["log.with.dots"],
                    "type": "console",
                    "encoding": {
                        "codec": "json"
                    }
                }
            }
        }"#,
        );
        let errors = builder.build().unwrap_err();
        assert_eq!(
            errors[0],
            "Component name \"log.with.dots\" should not contain a \".\""
        );
    }

    #[test]
    fn pipeline_component_with_dot() {
        let mut pipelines = IndexMap::new();
        pipelines.insert(
            "foo".into(),
            Pipeline::from_json(
                r#"
        {
            "transforms": {
                "remap.with.dots": {
                    "inputs": ["logs"],
                    "type": "remap",
                    "source": "",
                    "outputs": ["print"]
                }
            }
        }
        "#,
            ),
        );
        let pipelines = Pipelines::from(pipelines);
        let mut builder = ConfigBuilder::from_toml(
            r#"
        [sources.logs]
        type = "generator"
        format = "syslog"

        [sinks.print]
        type = "console"
        encoding.codec = "json"
        "#,
        );
        builder.set_pipelines(pipelines);
        let errors = builder.build().unwrap_err();
        assert_eq!(
            errors[0],
            "Component name \"remap.with.dots\" should not contain a \".\""
        );
    }

    #[test]
    fn pipeline_with_dot() {
        let mut pipelines = IndexMap::new();
        pipelines.insert(
            "foo.bar".into(),
            Pipeline::from_toml(
                r#"
        [transforms.remap]
        inputs = ["logs"]
        type = "remap"
        source = ""
        outputs = ["print"]
        "#,
            ),
        );
        let pipelines = Pipelines::from(pipelines);
        let mut builder = ConfigBuilder::from_toml(
            r#"
        [sources.logs]
        type = "generator"
        format = "syslog"

        [sinks.print]
        type = "console"
        encoding.codec = "json"
        "#,
        );
        builder.set_pipelines(pipelines);
        let errors = builder.build().unwrap_err();
        assert_eq!(
            errors[0],
            "Pipeline name \"foo.bar\" shouldn't container a '.'."
        );
    }
}
