use serde::{self, Deserialize, Serialize};
use vector_core::transform::{InnerTopology, InnerTopologyTransform};

use crate::{
    config::{
        ComponentKey, DataType, GenerateConfig, Input, Output, TransformConfig, TransformContext,
        TransformDescription,
    },
    schema,
    transforms::Transform,
};

#[derive(Deserialize, Serialize, Debug, Clone)]
struct CompoundConfig {
    steps: Vec<TransformStep>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct TransformStep {
    id: Option<String>,

    #[serde(flatten)]
    transform: Box<dyn TransformConfig>,
}

impl TransformStep {
    pub fn id(&self, index: usize) -> String {
        self.id
            .as_ref()
            .cloned()
            .unwrap_or_else(|| index.to_string())
    }
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
        name: &ComponentKey,
        inputs: &[String],
    ) -> crate::Result<Option<InnerTopology>> {
        let definition = schema::Definition::empty();

        let last_step = self
            .steps
            .last()
            .ok_or("must specify at least one transform")?;
        let mut result = InnerTopology {
            inner: Default::default(),
            outputs: vec![(
                name.join(last_step.id(self.steps.len() - 1)),
                last_step.transform.outputs(&definition),
            )],
        };

        let mut last_inputs = inputs.to_vec();
        for (i, step) in self.steps.iter().enumerate() {
            let step_name = name.join(step.id(i));

            let step_outputs = step
                .transform
                .outputs(&definition)
                .into_iter()
                .map(|output| {
                    output
                        .port
                        .map(|p| step_name.port(p))
                        .unwrap_or_else(|| step_name.id().to_string())
                })
                .collect::<Vec<String>>();

            let inner_transform = InnerTopologyTransform {
                inputs: last_inputs,
                inner: step.transform.to_owned(),
            };

            if result
                .inner
                .insert(step_name.clone(), inner_transform)
                .is_some()
            {
                return Err("conflicting id found while expanding transform".into());
            }

            last_inputs = step_outputs;
        }

        if !result.inner.is_empty() {
            Ok(Some(result))
        } else {
            Err("must specify at least one transform".into())
        }
    }

    fn input(&self) -> Input {
        Input::all()
    }

    fn outputs(&self, _: &schema::Definition) -> Vec<Output> {
        vec![Output::default(DataType::all())]
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
        let name = ComponentKey::from("main");
        let inputs = vec!["bar".to_owned(), "baz".to_owned()];
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
        .expand(&name, &inputs)
        .unwrap()
        .unwrap();

        assert_eq!(
            serde_json::to_string(&config.inner).unwrap(),
            r#"{"main.0":{"inputs":["bar","baz"],"inner":{"type":"mock"}},"main.foo":{"inputs":["main.0"],"inner":{"type":"mock"}}}"#,
        );
        assert_eq!(
            config.outputs,
            vec![(
                ComponentKey::from("main.foo"),
                vec![Output::default(DataType::all())]
            )]
        );
    }
}
