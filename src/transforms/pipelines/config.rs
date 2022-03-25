use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use vector_core::transform::{InnerTopology, InnerTopologyTransform};

use crate::{
    conditions::AnyCondition,
    config::{ComponentKey, DataType, Output, TransformConfig},
    transforms::route::{RouteConfig, UNMATCHED_ROUTE},
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
    pub(super) fn expand(
        &mut self,
        name: &ComponentKey,
        inputs: &[String],
    ) -> crate::Result<Option<InnerTopology>> {
        let mut result = InnerTopology::default();
        // define the name of the last output
        let last_name = if self.transforms.is_empty() {
            self.filter
                .as_ref()
                .map(|_filter| {
                    let filter_name = name.join("filter");
                    filter_name.join("success")
                })
                .ok_or_else(|| "mut have at least one transform or a filter".to_string())?
        } else {
            name.join(self.transforms.len() - 1)
        };
        result
            .outputs
            .push((last_name, vec![Output::default(DataType::all())]));
        // insert the filter if needed and return the next inputs
        let mut next_inputs = if let Some(ref filter) = self.filter {
            let mut conditions = IndexMap::new();
            conditions.insert("success".to_string(), filter.to_owned());
            let filter_name = name.join("filter");
            result.inner.insert(
                filter_name.clone(),
                InnerTopologyTransform {
                    inputs: inputs.to_vec(),
                    inner: Box::new(RouteConfig::new(conditions)),
                },
            );
            result.outputs.push((
                filter_name.clone(),
                vec![Output::from((UNMATCHED_ROUTE, DataType::all()))],
            ));
            vec![filter_name.port("success")]
        } else {
            inputs.to_vec()
        };
        // compound like
        for (index, transform) in self.transforms.iter().enumerate() {
            let step_name = name.join(index);
            result.inner.insert(
                step_name.clone(),
                InnerTopologyTransform {
                    inputs: next_inputs,
                    inner: transform.to_owned(),
                },
            );
            next_inputs = vec![step_name.id().to_string()];
        }
        //
        Ok(Some(result))
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
    pub(super) fn validate_nesting(&self, parents: &HashSet<&'static str>) -> Result<(), String> {
        for (name, pipeline) in self.pipelines.iter() {
            for (index, transform) in pipeline.transforms.iter().enumerate() {
                if !transform.nestable(parents) {
                    return Err(format!(
                        "the transform {} in pipeline {:?} cannot be nested in {:?}",
                        index, name, parents
                    ));
                }
            }
        }
        Ok(())
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
    pub(super) fn expand(
        &mut self,
        name: &ComponentKey,
        inputs: &[String],
    ) -> crate::Result<Option<InnerTopology>> {
        let mut result = InnerTopology::default();
        let mut next_inputs = inputs.to_vec();
        for pipe_name in self.names() {
            let pipeline_name = name.join(&pipe_name);
            let pipeline_config = self
                .pipelines
                .get_mut(&pipe_name)
                .ok_or_else(|| format!("Unable to find pipeline with name {:?}", pipe_name))?;
            let topology = pipeline_config
                .expand(&pipeline_name, &next_inputs)?
                .ok_or_else(|| format!("Unable to expand pipeline named {:?}", pipe_name))?;
            result.inner.extend(topology.inner.into_iter());
            result.outputs = topology.outputs;
            next_inputs = result.outputs();
        }
        //
        Ok(Some(result))
    }
}
