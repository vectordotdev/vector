use indexmap::IndexMap;
use serde::{self, Deserialize, Serialize};

use crate::{
    config::{
        DataType, ExpandType, GenerateConfig, TransformConfig, TransformContext,
        TransformDescription,
    },
    transforms::Transform,
};

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct CompoundConfig {
    steps: Vec<TransformStep>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct TransformStep {
    id: Option<String>,

    #[serde(flatten)]
    transform: Box<dyn TransformConfig>,
}

inventory::submit! {
    TransformDescription::new::<CompoundConfig>("compound")
}

impl GenerateConfig for CompoundConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self { steps: Vec::new() }).unwrap()
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
        let mut map: IndexMap<String, Box<dyn TransformConfig>> = IndexMap::new();
        for (i, step) in self.steps.iter().enumerate() {
            if map
                .insert(
                    step.id.as_ref().cloned().unwrap_or_else(|| i.to_string()),
                    step.transform.to_owned(),
                )
                .is_some()
            {
                return Err("conflicting id found while expanding transform".into());
            }
        }

        if !map.is_empty() {
            Ok(Some((map, ExpandType::Serial { alias: false })))
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
            [[steps]]
            type = "mock"
            suffix = "step1"

            [[steps]]
            type = "mock"
            id = "foo"
            suffix = "step1"
        "#,
        )
        .unwrap()
        .expand()
        .unwrap()
        .unwrap();

        assert_eq!(
            serde_json::to_string(&config).unwrap(),
            r#"[{"0":{"type":"mock"},"foo":{"type":"mock"}},{"Serial":{"alias":false}}]"#
        );
    }
}
