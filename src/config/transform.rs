use std::collections::{HashMap, HashSet};

use async_trait::async_trait;
use enum_dispatch::enum_dispatch;
use indexmap::IndexMap;
use serde::Serialize;
use vector_config::{configurable_component, Configurable, NamedComponent};
use vector_core::{
    config::{GlobalOptions, Input, Output},
    schema,
    transform::Transform,
};

use crate::transforms::Transforms;

use super::ComponentKey;

/// Fully resolved transform component.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct TransformOuter<T>
where
    T: Configurable + Serialize,
{
    /// Inputs to the transforms.
    #[serde(default = "Default::default")] // https://github.com/serde-rs/serde/issues/1541
    pub inputs: Vec<T>,

    #[serde(flatten)]
    pub inner: Transforms,
}

impl<T> TransformOuter<T>
where
    T: Configurable + Serialize,
{
    pub(crate) fn new<I: Into<Transforms>>(inputs: Vec<T>, inner: I) -> Self {
        TransformOuter {
            inputs,
            inner: inner.into(),
        }
    }

    pub(super) fn map_inputs<U>(self, f: impl Fn(&T) -> U) -> TransformOuter<U>
    where
        U: Configurable + Serialize,
    {
        let inputs = self.inputs.iter().map(f).collect();
        self.with_inputs(inputs)
    }

    pub(crate) fn with_inputs<U>(self, inputs: Vec<U>) -> TransformOuter<U>
    where
        U: Configurable + Serialize,
    {
        TransformOuter {
            inputs,
            inner: self.inner,
        }
    }
}

impl TransformOuter<String> {
    pub(crate) fn expand(
        mut self,
        key: ComponentKey,
        parent_types: &HashSet<&'static str>,
        transforms: &mut IndexMap<ComponentKey, TransformOuter<String>>,
        expansions: &mut IndexMap<ComponentKey, Vec<ComponentKey>>,
    ) -> Result<(), String> {
        if !self.inner.nestable(parent_types) {
            return Err(format!(
                "the component {} cannot be nested in {:?}",
                self.inner.get_component_name(),
                parent_types
            ));
        }

        let expansion = self
            .inner
            .expand(&key, &self.inputs)
            .map_err(|err| format!("failed to expand transform '{}': {}", key, err))?;

        let mut ptypes = parent_types.clone();
        ptypes.insert(self.inner.get_component_name());

        if let Some(inner_topology) = expansion {
            let mut children = Vec::new();

            expansions.insert(
                key,
                inner_topology
                    .outputs()
                    .into_iter()
                    .map(ComponentKey::from)
                    .collect(),
            );

            for (inner_name, inner_transform) in inner_topology.inner {
                let child = TransformOuter {
                    inputs: inner_transform.inputs,
                    inner: inner_transform.inner,
                };
                children.push(inner_name.clone());
                transforms.insert(inner_name, child);
            }
        } else {
            transforms.insert(key, self);
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct TransformContext {
    // This is optional because currently there are a lot of places we use `TransformContext` that
    // may not have the relevant data available (e.g. tests). In the future it'd be nice to make it
    // required somehow.
    pub key: Option<ComponentKey>,

    pub globals: GlobalOptions,

    pub enrichment_tables: enrichment::TableRegistry,

    /// Tracks the schema IDs assigned to schemas exposed by the transform.
    ///
    /// Given a transform can expose multiple [`Output`] channels, the ID is tied to the identifier of
    /// that `Output`.
    pub schema_definitions: HashMap<Option<String>, schema::Definition>,

    /// The schema definition created by merging all inputs of the transform.
    ///
    /// This information can be used by transforms that behave differently based on schema
    /// information, such as the `remap` transform, which passes this information along to the VRL
    /// compiler such that type coercion becomes less of a need for operators writing VRL programs.
    pub merged_schema_definition: schema::Definition,
}

impl Default for TransformContext {
    fn default() -> Self {
        Self {
            key: Default::default(),
            globals: Default::default(),
            enrichment_tables: Default::default(),
            schema_definitions: HashMap::from([(None, schema::Definition::any())]),
            merged_schema_definition: schema::Definition::any(),
        }
    }
}

impl TransformContext {
    // clippy allow avoids an issue where vrl is flagged off and `globals` is
    // the sole field in the struct
    #[allow(clippy::needless_update)]
    pub fn new_with_globals(globals: GlobalOptions) -> Self {
        Self {
            globals,
            ..Default::default()
        }
    }

    #[cfg(any(test, feature = "test"))]
    pub fn new_test(schema_definitions: HashMap<Option<String>, schema::Definition>) -> Self {
        Self {
            schema_definitions,
            ..Default::default()
        }
    }
}

#[async_trait]
#[enum_dispatch]
pub trait TransformConfig: NamedComponent + core::fmt::Debug + Send + Sync {
    async fn build(&self, globals: &TransformContext) -> crate::Result<Transform>;

    fn input(&self) -> Input;

    /// Returns a list of outputs to which this transform can deliver events.
    ///
    /// The provided `merged_definition` can be used by transforms to understand the expected shape
    /// of events flowing through the transform.
    fn outputs(&self, merged_definition: &schema::Definition) -> Vec<Output>;

    /// Verifies that the provided outputs and the inner plumbing of the transform are valid.
    fn validate(&self, _merged_definition: &schema::Definition) -> Result<(), Vec<String>> {
        Ok(())
    }

    /// Return true if the transform is able to be run across multiple tasks simultaneously with no
    /// concerns around statefulness, ordering, etc.
    fn enable_concurrency(&self) -> bool {
        false
    }

    /// Allows to detect if a transform can be embedded in another transform.
    /// It's used by the pipelines transform for now.
    fn nestable(&self, _parents: &HashSet<&'static str>) -> bool {
        true
    }

    /// Allows a transform configuration to expand itself into multiple "child"
    /// transformations to replace it. This allows a transform to act as a macro
    /// for various patterns.
    fn expand(
        &mut self,
        _name: &ComponentKey,
        _inputs: &[String],
    ) -> crate::Result<Option<InnerTopology>> {
        Ok(None)
    }
}

#[derive(Debug, Serialize)]
pub struct InnerTopologyTransform {
    pub inputs: Vec<String>,
    pub inner: Transforms,
}

#[derive(Debug, Default)]
pub struct InnerTopology {
    pub inner: IndexMap<ComponentKey, InnerTopologyTransform>,
    pub outputs: Vec<(ComponentKey, Vec<Output>)>,
}

impl InnerTopology {
    pub fn outputs(&self) -> Vec<String> {
        self.outputs
            .iter()
            .flat_map(|(name, outputs)| {
                outputs.iter().map(|output| match output.port {
                    Some(ref port) => name.port(port),
                    None => name.id().to_string(),
                })
            })
            .collect()
    }
}
