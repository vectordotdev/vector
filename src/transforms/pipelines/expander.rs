use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::{
    config::{DataType, ExpandType, TransformConfig, TransformContext},
    transforms::Transform,
};

/// This transform is a simple helper to chain expansions.
/// You can put a list of transforms that expands in parallel inside a transform that
/// expands in serial.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExpanderConfig {
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
        if self.inner.is_empty() {
            Err("must specify at least one transform".into())
        } else {
            Ok(Some((self.inner.clone(), self.mode.clone())))
        }
    }

    fn input_type(&self) -> DataType {
        self.inner
            .first()
            .map(|(_, item)| item.input_type())
            .unwrap_or(DataType::Any)
    }

    fn output_type(&self) -> DataType {
        self.inner
            .last()
            .map(|(_, item)| item.output_type())
            .unwrap_or(DataType::Any)
    }

    fn transform_type(&self) -> &'static str {
        "pipeline_expander"
    }
}
