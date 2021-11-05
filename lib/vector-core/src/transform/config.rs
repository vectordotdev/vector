use crate::config::GlobalOptions;
use async_trait::async_trait;
use indexmap::IndexMap;

#[derive(Debug, Clone, PartialEq, Copy)]
pub enum DataType {
    Any,
    Log,
    Metric,
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Debug)]
pub enum ExpandType {
    /// This way of expanding will duplicate the inputs for every expanded node.
    /// If `aggregates` is set to `true`, then a `Noop` transform will be added
    /// so that you can use the original component name as an input.
    Parallel { aggregates: bool },
    /// This ways of expanding will take all the components and chain then in order.
    /// The first node will be renamed `component_name.0` and so on.
    /// If `alias` is set to `true, then a `Noop` transform will be added as the
    /// last component and named `component_name` so that it can be used as an input.
    Serial { alias: bool },
}

#[cfg(feature = "vrl")]
#[derive(Debug, Default)]
pub struct TransformContext {
    pub globals: GlobalOptions,
    pub enrichment_tables: enrichment::TableRegistry,
}

#[cfg(not(feature = "vrl"))]
#[derive(Debug, Default)]
pub struct TransformContext {
    pub globals: GlobalOptions,
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
}

#[async_trait]
#[typetag::serde(tag = "type")]
pub trait TransformConfig: core::fmt::Debug + Send + Sync + dyn_clone::DynClone {
    async fn build(&self, globals: &TransformContext)
        -> crate::Result<crate::transform::Transform>;

    fn input_type(&self) -> DataType;

    fn output_type(&self) -> DataType;

    fn named_outputs(&self) -> Vec<String> {
        Vec::new()
    }

    fn transform_type(&self) -> &'static str;

    /// Allows a transform configuration to expand itself into multiple "child"
    /// transformations to replace it. This allows a transform to act as a macro
    /// for various patterns.
    fn expand(
        &mut self,
    ) -> crate::Result<Option<(IndexMap<String, Box<dyn TransformConfig>>, ExpandType)>> {
        Ok(None)
    }
}

dyn_clone::clone_trait_object!(TransformConfig);
