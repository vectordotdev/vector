use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::{
    config::{DataType, ExpandType, Input, Output, TransformConfig, TransformContext},
    schema,
    transforms::Transform,
};

/// This transform is a simple helper to chain expansions.
/// You can put a list of transforms that expands in parallel inside a transform that
/// expands in serial.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ExpanderConfig {
    mode: ExpandType,
    inner: IndexMap<String, Box<dyn TransformConfig>>,
}

impl ExpanderConfig {
    pub fn serial(inner: IndexMap<String, Box<dyn TransformConfig>>) -> Self {
        Self {
            mode: ExpandType::Serial { alias: true },
            inner,
        }
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "pipeline_expander")]
impl TransformConfig for ExpanderConfig {
    async fn build(&self, _context: &TransformContext) -> crate::Result<Transform> {
        Err("this transform must be expanded".into())
    }

    fn expand(
        &mut self,
    ) -> crate::Result<Option<(IndexMap<String, Box<dyn TransformConfig>>, ExpandType)>> {
        Ok(Some((self.inner.clone(), self.mode)))
    }

    fn input(&self) -> Input {
        self.inner
            .first()
            .map(|(_, item)| item.input())
            .unwrap_or_else(Input::all)
    }

    fn outputs(&self, merged_definition: &schema::Definition) -> Vec<Output> {
        self.inner
            .last()
            .map(|(_, item)| item.outputs(merged_definition))
            .unwrap_or_else(|| vec![Output::default(DataType::all())])
    }

    fn transform_type(&self) -> &'static str {
        "pipeline_expander"
    }
}
