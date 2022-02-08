use serde::{Deserialize, Serialize};

use crate::{
    config::{DataType, Output, TransformConfig, TransformContext},
    event::Event,
    transforms::{FunctionTransform, OutputBuffer, Transform},
};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Noop;

#[async_trait::async_trait]
#[typetag::serde(name = "noop")]
impl TransformConfig for Noop {
    async fn build(&self, _context: &TransformContext) -> crate::Result<Transform> {
        Ok(Transform::function(self.clone()))
    }

    fn input_type(&self) -> DataType {
        DataType::Any
    }

    fn outputs(&self) -> Vec<Output> {
        vec![Output::default(DataType::Any)]
    }

    fn transform_type(&self) -> &'static str {
        "noop"
    }
}

impl FunctionTransform for Noop {
    fn transform(&mut self, output: &mut OutputBuffer, event: Event) {
        output.push(event);
    }
}
