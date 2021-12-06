use crate::{
    config::{DataType, GenerateConfig, TransformConfig, TransformContext, TransformDescription},
    transforms::{noop::Noop, Transform},
};
use indexmap::IndexMap;
use serde::{self, Deserialize, Serialize};
use vector_core::config::ComponentKey;

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
        component_key: &ComponentKey,
        inputs: &[String],
    ) -> crate::Result<Option<IndexMap<ComponentKey, (Vec<String>, Box<dyn TransformConfig>)>>>
    {
        let mut map: IndexMap<ComponentKey, (Vec<String>, Box<dyn TransformConfig>)> =
            IndexMap::new();
        let mut previous: Vec<String> = inputs.into();

        for (i, step) in self.steps.iter().enumerate() {
            let step_key = if let Some(ref id) = step.id {
                component_key.join(id)
            } else {
                component_key.join(i)
            };
            if map
                .insert(step_key.clone(), (previous, step.transform.to_owned()))
                .is_some()
            {
                return Err("conflicting id found while expanding transform".into());
            }
            previous = vec![step_key.id().to_owned()];
        }

        map.insert(component_key.clone(), (previous, Box::new(Noop)));

        if !map.is_empty() {
            Ok(Some(map))
        } else {
            Err("must specify at least one transform".into())
        }
    }

    fn named_outputs(&self) -> Vec<(Option<String>, DataType)> {
        let mut result = vec![(None, self.output_type())];
        for (index, step) in self.steps.iter().enumerate() {
            let id = step.id.clone().unwrap_or_else(|| index.to_string());
            result.push((Some(id), step.transform.output_type()));
        }
        result
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
        let mut config = toml::from_str::<CompoundConfig>(
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
        .unwrap();
        assert_eq!(config.named_outputs().len(), 3);

        let root = ComponentKey::from("root");
        let inputs = vec!["bar".to_owned()];
        let config = config.expand(&root, &inputs).unwrap().unwrap();

        assert_eq!(
            serde_json::to_string(&config).unwrap(),
            r#"{"root.0":[["bar"],{"type":"mock"}],"root.foo":[["root.0"],{"type":"mock"}],"root":[["root.foo"],{"type":"noop"}]}"#
        );
    }
}
