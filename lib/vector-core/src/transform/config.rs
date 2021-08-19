use crate::config::GlobalOptions;
use crate::enrichment;
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

#[derive(Debug)]
pub struct TransformContext {
    pub globals: GlobalOptions,
    pub enrichment_tables: enrichment::TableRegistry,
}

impl TransformContext {
    pub fn new_with_globals(globals: GlobalOptions) -> Self {
        Self {
            globals,
            enrichment_tables: Default::default(),
        }
    }
}

impl Default for TransformContext {
    fn default() -> Self {
        TransformContext {
            globals: Default::default(),
            enrichment_tables: Default::default(),
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
