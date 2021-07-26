use crate::{
    config::{
        DataType, ExpandType, GenerateConfig, GlobalOptions, TransformConfig, TransformDescription,
    },
    transforms::Transform,
};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct CompoundConfig {
    nested: IndexMap<String, Box<dyn TransformConfig>>,
}

inventory::submit! {
    TransformDescription::new::<CompoundConfig>("compound")
}

impl GenerateConfig for CompoundConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            nested: IndexMap::new(),
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "compound")]
impl TransformConfig for CompoundConfig {
    async fn build(&self, _globals: &GlobalOptions) -> crate::Result<Transform> {
        Err("this transform must be expanded".into())
    }

    fn expand(
        &mut self,
    ) -> crate::Result<Option<(IndexMap<String, Box<dyn TransformConfig>>, ExpandType)>> {
        if !self.nested.is_empty() {
            // Expand transform into a sequence of transforms
            debug!("{:?}", self.nested);
            Ok(Some((self.nested.clone(), ExpandType::Serial)))
        } else {
            Err("must specify at least one transform".into())
        }
    }

    fn input_type(&self) -> DataType {
        DataType::Any
    }

    fn output_type(&self) -> DataType {
        DataType::Any
    }

    fn transform_type(&self) -> &'static str {
        "compound"
    }
}
