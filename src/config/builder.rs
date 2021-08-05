#[cfg(feature = "api")]
use super::api;
use super::Pipelines;
use super::{
    compiler, provider, Config, HealthcheckOptions, SinkConfig, SinkOuter, SourceConfig,
    SourceOuter, TestDefinition, TransformOuter,
};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use vector_core::config::GlobalOptions;
use vector_core::default_data_dir;
use vector_core::transform::TransformConfig;

#[derive(Deserialize, Serialize, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct ConfigBuilder {
    #[serde(flatten)]
    pub global: GlobalOptions,
    #[cfg(feature = "api")]
    #[serde(default)]
    pub api: api::Options,
    #[serde(default)]
    pub healthchecks: HealthcheckOptions,
    #[serde(default)]
    pub sources: IndexMap<String, SourceOuter>,
    #[serde(default)]
    pub sinks: IndexMap<String, SinkOuter>,
    #[serde(default)]
    pub transforms: IndexMap<String, TransformOuter>,
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
    fn from(c: Config) -> Self {
        ConfigBuilder {
            global: c.global,
            #[cfg(feature = "api")]
            api: c.api,
            healthchecks: c.healthchecks,
            sources: c.sources,
            sinks: c.sinks,
            transforms: c.transforms,
            provider: None,
            tests: c.tests,
        }
    }
}

impl ConfigBuilder {
    pub fn build(self, pipelines: Pipelines) -> Result<Config, Vec<String>> {
        let (config, warnings) = self.build_with_warnings(pipelines)?;

        for warning in warnings {
            warn!("{}", warning);
        }

        Ok(config)
    }

    pub fn build_with_warnings(
        self,
        pipelines: Pipelines,
    ) -> Result<(Config, Vec<String>), Vec<String>> {
        compiler::compile(self, pipelines)
    }

    pub fn add_source<S: SourceConfig + 'static, T: Into<String>>(&mut self, name: T, source: S) {
        self.sources.insert(name.into(), SourceOuter::new(source));
    }

    pub fn add_sink<S: SinkConfig + 'static, T: Into<String>>(
        &mut self,
        name: T,
        inputs: &[&str],
        sink: S,
    ) {
        let inputs = inputs.iter().map(|&s| s.to_owned()).collect::<Vec<_>>();
        let sink = SinkOuter::new(inputs, Box::new(sink));

        self.sinks.insert(name.into(), sink);
    }

    pub fn add_transform<T: TransformConfig + 'static, S: Into<String>>(
        &mut self,
        name: S,
        inputs: &[&str],
        transform: T,
    ) {
        let inputs = inputs.iter().map(|&s| s.to_owned()).collect::<Vec<_>>();
        let transform = TransformOuter {
            inner: Box::new(transform),
            inputs,
        };

        self.transforms.insert(name.into(), transform);
    }

    pub fn append(&mut self, with: Self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        #[cfg(feature = "api")]
        if let Err(error) = self.api.merge(with.api) {
            errors.push(error);
        }

        self.provider = with.provider;

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

        with.sources.keys().for_each(|k| {
            if self.sources.contains_key(k) {
                errors.push(format!("duplicate source name found: {}", k));
            }
        });
        with.sinks.keys().for_each(|k| {
            if self.sinks.contains_key(k) {
                errors.push(format!("duplicate sink name found: {}", k));
            }
        });
        with.transforms.keys().for_each(|k| {
            if self.transforms.contains_key(k) {
                errors.push(format!("duplicate transform name found: {}", k));
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

        self.sources.extend(with.sources);
        self.sinks.extend(with.sinks);
        self.transforms.extend(with.transforms);
        self.tests.extend(with.tests);

        Ok(())
    }
}

// Related to validation
impl ConfigBuilder {
    pub(super) fn component_names(&self) -> HashMap<&str, Vec<&'static str>> {
        let mut name_uses = HashMap::<&str, Vec<&'static str>>::new();
        for (ctype, name) in tagged("source", self.sources.keys())
            .chain(tagged("transform", self.transforms.keys()))
            .chain(tagged("sink", self.sinks.keys()))
        {
            let uses = name_uses.entry(name).or_default();
            uses.push(ctype);
        }
        name_uses
    }

    // Check for non-unique names across sources, sinks, and transforms
    fn check_conflicts(&self, pipelines: &Pipelines, errors: &mut Vec<String>) {
        let name_uses = self.component_names();
        for (name, uses) in name_uses.iter().filter(|(_name, uses)| uses.len() > 1) {
            errors.push(format!(
                "More than one component with name {:?} ({}).",
                name,
                uses.join(", ")
            ));
        }

        pipelines.check_conflicts(&name_uses, errors);
    }

    // Check that sinks and transforms have inputs and that thoses inputs exist
    fn check_inputs(&self, pipelines: &Pipelines, errors: &mut Vec<String>) {
        let sink_inputs = self
            .sinks
            .iter()
            .map(|(name, sink)| ("sink", name.clone(), sink.inputs.clone()));
        let transform_inputs = self
            .transforms
            .iter()
            .map(|(name, transform)| ("transform", name.clone(), transform.inputs.clone()));
        let pipeline_outputs: HashSet<_> = pipelines.outputs().collect();
        for (output_type, name, inputs) in sink_inputs.chain(transform_inputs) {
            if inputs.is_empty() && !pipeline_outputs.contains(&name) {
                errors.push(format!(
                    "{} {:?} has no inputs",
                    capitalize(output_type),
                    name
                ));
            }

            for input in inputs {
                if !self.has_input(&input) {
                    errors.push(format!(
                        "Input {:?} for {} {:?} doesn't exist.",
                        input, output_type, name
                    ));
                }
            }
        }
    }

    pub(super) fn check_shape(&self, pipelines: &Pipelines) -> Result<(), Vec<String>> {
        let mut errors = vec![];

        if self.sources.is_empty() {
            errors.push("No sources defined in the config.".to_owned());
        }

        if self.sinks.is_empty() {
            errors.push("No sinks defined in the config.".to_owned());
        }

        self.check_conflicts(pipelines, &mut errors);

        pipelines.check_shape(&self, &mut errors);

        self.check_inputs(pipelines, &mut errors);

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    pub(super) fn has_input(&self, name: &str) -> bool {
        self.sources.contains_key(name) || self.transforms.contains_key(name)
    }

    pub(super) fn has_output(&self, name: &str) -> bool {
        self.transforms.contains_key(name) || self.sinks.contains_key(name)
    }
}

fn capitalize(s: &str) -> String {
    let mut s = s.to_owned();
    if let Some(r) = s.get_mut(0..1) {
        r.make_ascii_uppercase();
    }
    s
}

fn tagged<'a>(
    tag: &'static str,
    iter: impl Iterator<Item = &'a String>,
) -> impl Iterator<Item = (&'static str, &'a String)> {
    iter.map(move |x| (tag, x))
}
