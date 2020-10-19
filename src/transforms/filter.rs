use crate::{
    conditions::{AnyCondition, Condition},
    config::{DataType, GenerateConfig, TransformConfig, TransformContext, TransformDescription},
    event::Event,
    transforms::{FunctionTransform, Transform},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct FilterConfig {
    condition: AnyCondition,
}

inventory::submit! {
    TransformDescription::new::<FilterConfig>("filter")
}

impl GenerateConfig for FilterConfig {}

#[async_trait::async_trait]
#[typetag::serde(name = "filter")]
impl TransformConfig for FilterConfig {
    async fn build(&self, _cx: TransformContext) -> crate::Result<Transform> {
        Ok(Transform::function(Filter::new(self.condition.build()?)))
    }

    fn input_type(&self) -> DataType {
        DataType::Any
    }

    fn output_type(&self) -> DataType {
        DataType::Any
    }

    fn transform_type(&self) -> &'static str {
        "filter"
    }
}

pub struct Filter {
    condition: Box<dyn Condition>,
}

impl Filter {
    pub fn new(condition: Box<dyn Condition>) -> Self {
        Self { condition }
    }
}

impl FunctionTransform for Filter {
    fn transform(&mut self, output: &mut Vec<Event>, event: Event) {
        if self.condition.check(&event) {
            output.push(event);
        }
    }
}
