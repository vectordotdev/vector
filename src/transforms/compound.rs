use crate::{
    config::{
        DataType, ExpandType, GenerateConfig, TransformConfig, TransformContext,
        TransformDescription,
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
    async fn build(&self, _context: &TransformContext) -> crate::Result<Transform> {
        Err("this transform must be expanded".into())
    }

    fn expand(
        &mut self,
    ) -> crate::Result<Option<(IndexMap<String, Box<dyn TransformConfig>>, ExpandType)>> {
        if !self.nested.is_empty() {
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

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<super::CompoundConfig>();
    }

    #[test]
    fn can_serialize_nested_transforms() {
        // We need to serialize the config to check if a config has
        // changed when reloading.
        let config = toml::from_str::<CompoundConfig>(
            r#"
            [nested.step1]
            type = "mock"
            suffix = "step1"

            [nested.step2]
            type = "mock"
            suffix = "step1"
        "#,
        )
        .unwrap()
        .expand()
        .unwrap()
        .unwrap();

        assert_eq!(
            serde_json::to_string(&config).unwrap(),
            r#"[{"step1":{"type":"mock"},"step2":{"type":"mock"}},"Serial"]"#
        );
    }
}
