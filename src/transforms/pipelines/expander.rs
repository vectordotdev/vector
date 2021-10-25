use crate::config::{DataType, ExpandType, TransformConfig, TransformContext};
use crate::transforms::Transform;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExpanderConfig {
    mode: ExpandType,
    inner: IndexMap<String, Box<dyn TransformConfig>>,
}

impl ExpanderConfig {
    pub fn parallel(inner: IndexMap<String, Box<dyn TransformConfig>>) -> Self {
        Self {
            mode: ExpandType::Parallel,
            inner,
        }
    }

    pub fn serial(inner: IndexMap<String, Box<dyn TransformConfig>>) -> Self {
        Self {
            mode: ExpandType::Serial,
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
        DataType::Any
    }

    fn output_type(&self) -> DataType {
        DataType::Any
    }

    fn transform_type(&self) -> &'static str {
        "pipeline_expander"
    }
}
