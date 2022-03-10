use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::{
    conditions::AnyCondition,
    config::TransformConfig,
    transforms::pipelines::{expander, filter},
};

//------------------------------------------------------------------------------

/// This represents the configuration of a single pipeline, not the pipelines transform
/// itself, which can contain multiple individual pipelines
#[derive(Debug, Default, Deserialize, Serialize)]
pub(crate) struct PipelineConfig {
    name: String,
    filter: Option<AnyCondition>,
    #[serde(default)]
    transforms: Vec<Box<dyn TransformConfig>>,
}

#[cfg(test)]
impl PipelineConfig {
    #[allow(dead_code)] // for some small subset of feature flags this code is dead
    pub(crate) fn transforms(&self) -> &Vec<Box<dyn TransformConfig>> {
        &self.transforms
    }
}

impl Clone for PipelineConfig {
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

impl PipelineConfig {
    /// Expands a single pipeline into a series of its transforms.
    fn serial(&self) -> Box<dyn TransformConfig> {
        let transforms: IndexMap<String, Box<dyn TransformConfig>> = self
            .transforms
            .iter()
            .enumerate()
            .map(|(index, config)| (index.to_string(), config.clone()))
            .collect();
        let transforms = Box::new(expander::ExpanderConfig::serial(transforms));
        if let Some(ref filter) = self.filter {
            Box::new(filter::PipelineFilterConfig::new(
                filter.clone(),
                transforms,
            ))
        } else {
            transforms
        }
    }
}

//------------------------------------------------------------------------------

/// This represent an ordered list of pipelines depending on the event type.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EventTypeConfig {
    #[serde(default)]
    order: Option<Vec<String>>,
    pipelines: IndexMap<String, PipelineConfig>,
}

#[cfg(test)]
impl EventTypeConfig {
    #[allow(dead_code)] // for some small subset of feature flags this code is dead
    pub(crate) const fn order(&self) -> &Option<Vec<String>> {
        &self.order
    }

    #[allow(dead_code)] // for some small subset of feature flags this code is dead
    pub(crate) const fn pipelines(&self) -> &IndexMap<String, PipelineConfig> {
        &self.pipelines
    }
}

impl EventTypeConfig {
    pub(super) fn is_empty(&self) -> bool {
        self.pipelines.is_empty()
    }

    fn names(&self) -> Vec<String> {
        if let Some(ref names) = self.order {
            // This assumes all the pipelines are present in the `order` field.
            // If a pipeline is missing, it won't be used.
            names.clone()
        } else {
            let mut names = self.pipelines.keys().cloned().collect::<Vec<String>>();
            names.sort();
            names
        }
    }

    /// Expand sub-pipelines configurations, preserving user defined order
    ///
    /// This function expands the sub-pipelines according to the order passed by
    /// the user, or, absent an explicit order, by the position of the
    /// sub-pipeline in the configuration file.
    pub(super) fn expand(&self) -> IndexMap<String, Box<dyn TransformConfig>> {
        self.names()
            .into_iter()
            .filter_map(|name: String| {
                self.pipelines
                    .get(&name)
                    .map(|config: &PipelineConfig| (name, config.serial()))
            })
            .collect()
    }

    /// Expands a group of pipelines into a series of pipelines.
    /// They will then be expanded into a series of transforms.
    pub(super) fn serial(&self) -> Box<dyn TransformConfig> {
        let pipelines: IndexMap<String, Box<dyn TransformConfig>> = self.expand();
        Box::new(expander::ExpanderConfig::serial(pipelines))
    }
}
