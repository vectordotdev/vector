use std::collections::{HashMap, HashSet};

use async_trait::async_trait;
use enum_dispatch::enum_dispatch;
use serde::Serialize;
use vector_config::{configurable_component, Configurable, NamedComponent};
use vector_core::config::LogNamespace;
use vector_core::{
    config::{GlobalOptions, Input, Output},
    schema,
    transform::Transform,
};

use crate::transforms::Transforms;

use super::schema::Options as SchemaOptions;
use super::{id::Inputs, ComponentKey};

/// Fully resolved transform component.
#[configurable_component]
#[configurable(metadata(docs::component_base_type = "transform"))]
#[derive(Clone, Debug)]
pub struct TransformOuter<T>
where
    T: Configurable + Serialize,
{
    #[configurable(derived)]
    pub inputs: Inputs<T>,

    #[configurable(metadata(docs::hidden))]
    #[serde(flatten)]
    pub inner: Transforms,
}

impl<T> TransformOuter<T>
where
    T: Configurable + Serialize,
{
    pub(crate) fn new<I, IT>(inputs: I, inner: IT) -> Self
    where
        I: IntoIterator<Item = T>,
        IT: Into<Transforms>,
    {
        TransformOuter {
            inputs: Inputs::from_iter(inputs),
            inner: inner.into(),
        }
    }

    pub(super) fn map_inputs<U>(self, f: impl Fn(&T) -> U) -> TransformOuter<U>
    where
        U: Configurable + Serialize,
    {
        let inputs = self.inputs.iter().map(f).collect::<Vec<_>>();
        self.with_inputs(inputs)
    }

    pub(crate) fn with_inputs<I, U>(self, inputs: I) -> TransformOuter<U>
    where
        I: IntoIterator<Item = U>,
        U: Configurable + Serialize,
    {
        TransformOuter {
            inputs: Inputs::from_iter(inputs),
            inner: self.inner,
        }
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

    pub schema: SchemaOptions,
}

impl Default for TransformContext {
    fn default() -> Self {
        Self {
            key: Default::default(),
            globals: Default::default(),
            enrichment_tables: Default::default(),
            schema_definitions: HashMap::from([(None, schema::Definition::any())]),
            merged_schema_definition: schema::Definition::any(),
            schema: SchemaOptions::default(),
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

    /// Gets the log namespacing to use. The passed in value is from the transform itself
    /// and will override any global default if it's set.
    ///
    /// This should only be used for transforms that don't originate from a log (eg: `metric_to_log`)
    /// Most transforms will keep the log_namespace value that already exists on the event.
    pub fn log_namespace(&self, namespace: Option<bool>) -> LogNamespace {
        namespace
            .or(self.schema.log_namespace)
            .unwrap_or(false)
            .into()
    }
}

/// Generalized interface for describing and building transform components.
#[async_trait]
#[enum_dispatch]
pub trait TransformConfig: NamedComponent + core::fmt::Debug + Send + Sync {
    /// Builds the transform with the given context.
    ///
    /// If the transform is built successfully, `Ok(...)` is returned containing the transform.
    ///
    /// # Errors
    ///
    /// If an error occurs while building the transform, an error variant explaining the issue is
    /// returned.
    async fn build(&self, globals: &TransformContext) -> crate::Result<Transform>;

    /// Gets the input configuration for this transform.
    fn input(&self) -> Input;

    /// Gets the list of outputs exposed by this transform.
    ///
    /// The provided `merged_definition` can be used by transforms to understand the expected shape
    /// of events flowing through the transform.
    fn outputs(
        &self,
        merged_definition: &schema::Definition,
        global_log_namespace: LogNamespace,
    ) -> Vec<Output>;

    /// Validates that the configuration of the transform is valid.
    ///
    /// This would generally be where logical conditions were checked, such as ensuring a transform
    /// isn't using a named output that matches a reserved output name, and so on.
    ///
    /// # Errors
    ///
    /// If validation does not succeed, an error variant containing a list of all validation errors
    /// is returned.
    fn validate(&self, _merged_definition: &schema::Definition) -> Result<(), Vec<String>> {
        Ok(())
    }

    /// Whether or not concurrency should be enabled for this transform.
    ///
    /// When enabled, this transform may be run in parallel in order to attempt to maximize
    /// throughput for this node in the topology. Transforms should generally not run concurrently
    /// unless they are compute-heavy, as there is a cost/overhead associated with fanning out
    /// events to the parallel transform tasks.
    fn enable_concurrency(&self) -> bool {
        false
    }

    /// Whether or not this transform can be nested, given the types of transforms it would be
    /// nested within.
    ///
    /// For some transforms, they can expand themselves into a subtopology of nested transforms.
    /// However, in order to prevent an infinite recursion of nested transforms, we may want to only
    /// allow one layer of "expansion". Additionally, there may be known issues with a transform
    /// that is nested under another specific transform interacting poorly, or incorrectly.
    ///
    /// This method allows a transform to report if it can or cannot function correctly if it is
    /// nested under transforms of a specific type, or if such nesting is fundamentally disallowed.
    fn nestable(&self, _parents: &HashSet<&'static str>) -> bool {
        true
    }
}
