use crate::config::{ComponentKey, GlobalOptions};
use async_trait::async_trait;
use indexmap::IndexMap;

#[derive(Debug, Clone, PartialEq, Copy)]
pub enum DataType {
    Any,
    Log,
    Metric,
}

#[derive(serde::Deserialize, serde::Serialize, Debug)]
pub enum ExpandType {
    Parallel,
    Serial,
}

#[derive(Debug, Default)]
pub struct TransformContext {
    // This is optional because currently there are a lot of places we use `TransformContext` that
    // may not have the relevant data available (e.g. tests). In the future it'd be nice to make it
    // required somehow.
    pub key: Option<ComponentKey>,
    pub globals: GlobalOptions,
    #[cfg(feature = "vrl")]
    pub enrichment_tables: enrichment::TableRegistry,
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
