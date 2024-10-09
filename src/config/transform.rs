use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use async_trait::async_trait;
use dyn_clone::DynClone;
use serde::Serialize;
use vector_lib::configurable::attributes::CustomAttribute;
use vector_lib::configurable::{
    configurable_component,
    schema::{SchemaGenerator, SchemaObject},
    Configurable, GenerateError, Metadata, NamedComponent,
};
use vector_lib::{
    config::{GlobalOptions, Input, LogNamespace, TransformOutput},
    id::Inputs,
    schema,
    transform::Transform,
};

use super::dot_graph::GraphConfig;
use super::schema::Options as SchemaOptions;
use super::ComponentKey;
use super::OutputId;
use crate::extra_context::ExtraContext;

pub type BoxedTransform = Box<dyn TransformConfig>;

impl Configurable for BoxedTransform {
    fn referenceable_name() -> Option<&'static str> {
        Some("vector::transforms::Transforms")
    }

    fn metadata() -> Metadata {
        let mut metadata = Metadata::default();
        metadata.set_description("Configurable transforms in Vector.");
        metadata.add_custom_attribute(CustomAttribute::kv("docs::enum_tagging", "internal"));
        metadata.add_custom_attribute(CustomAttribute::kv("docs::enum_tag_field", "type"));
        metadata
    }

    fn generate_schema(gen: &RefCell<SchemaGenerator>) -> Result<SchemaObject, GenerateError> {
        vector_lib::configurable::component::TransformDescription::generate_schemas(gen)
    }
}

impl<T: TransformConfig + 'static> From<T> for BoxedTransform {
    fn from(that: T) -> Self {
        Box::new(that)
    }
}

/// Fully resolved transform component.
#[configurable_component]
#[configurable(metadata(docs::component_base_type = "transform"))]
#[derive(Clone, Debug)]
pub struct TransformOuter<T>
where
    T: Configurable + Serialize + 'static,
{
    #[configurable(derived)]
    #[serde(default, skip_serializing_if = "vector_lib::serde::is_default")]
    pub graph: GraphConfig,

    #[configurable(derived)]
    pub inputs: Inputs<T>,

    #[configurable(metadata(docs::hidden))]
    #[serde(flatten)]
    pub inner: BoxedTransform,
}

impl<T> TransformOuter<T>
where
    T: Configurable + Serialize,
{
    pub(crate) fn new<I, IT>(inputs: I, inner: IT) -> Self
    where
        I: IntoIterator<Item = T>,
        IT: Into<BoxedTransform>,
    {
        let inputs = Inputs::from_iter(inputs);
        let inner = inner.into();
        TransformOuter {
            inputs,
            inner,
            graph: Default::default(),
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
            graph: self.graph,
        }
    }
}

pub struct TransformContext {
    // This is optional because currently there are a lot of places we use `TransformContext` that
    // may not have the relevant data available (e.g. tests). In the future it'd be nice to make it
    // required somehow.
    pub key: Option<ComponentKey>,

    pub globals: GlobalOptions,

    pub enrichment_tables: vector_lib::enrichment::TableRegistry,

    pub vrl_caches: vector_lib::vrl_cache::VrlCacheRegistry,

    /// Tracks the schema IDs assigned to schemas exposed by the transform.
    ///
    /// Given a transform can expose multiple [`TransformOutput`] channels, the ID is tied to the identifier of
    /// that `TransformOutput`.
    pub schema_definitions: HashMap<Option<String>, HashMap<OutputId, schema::Definition>>,

    /// The schema definition created by merging all inputs of the transform.
    ///
    /// This information can be used by transforms that behave differently based on schema
    /// information, such as the `remap` transform, which passes this information along to the VRL
    /// compiler such that type coercion becomes less of a need for operators writing VRL programs.
    pub merged_schema_definition: schema::Definition,

    pub schema: SchemaOptions,

    /// Extra context data provided by the running app and shared across all components. This can be
    /// used to pass shared settings or other data from outside the components.
    pub extra_context: ExtraContext,
}

impl Default for TransformContext {
    fn default() -> Self {
        Self {
            key: Default::default(),
            globals: Default::default(),
            enrichment_tables: Default::default(),
            vrl_caches: Default::default(),
            schema_definitions: HashMap::from([(None, HashMap::new())]),
            merged_schema_definition: schema::Definition::any(),
            schema: SchemaOptions::default(),
            extra_context: Default::default(),
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
    pub fn new_test(
        schema_definitions: HashMap<Option<String>, HashMap<OutputId, schema::Definition>>,
    ) -> Self {
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
#[typetag::serde(tag = "type")]
pub trait TransformConfig: DynClone + NamedComponent + core::fmt::Debug + Send + Sync {
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
        enrichment_tables: vector_lib::enrichment::TableRegistry,
        vrl_caches: vector_lib::vrl_cache::VrlCacheRegistry,
        input_definitions: &[(OutputId, schema::Definition)],

        // This only exists for transforms that create logs from non-logs, to know which namespace
        // to use, such as `metric_to_log`
        global_log_namespace: LogNamespace,
    ) -> Vec<TransformOutput>;

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

dyn_clone::clone_trait_object!(TransformConfig);

/// Often we want to call outputs just to retrieve the OutputId's without needing
/// the schema definitions.
pub fn get_transform_output_ids<T: TransformConfig + ?Sized>(
    transform: &T,
    key: ComponentKey,
    global_log_namespace: LogNamespace,
) -> impl Iterator<Item = OutputId> + '_ {
    transform
        .outputs(
            vector_lib::enrichment::TableRegistry::default(),
            vector_lib::vrl_cache::VrlCacheRegistry::default(),
            &[(key.clone().into(), schema::Definition::any())],
            global_log_namespace,
        )
        .into_iter()
        .map(move |output| OutputId {
            component: key.clone(),
            port: output.port,
        })
}
