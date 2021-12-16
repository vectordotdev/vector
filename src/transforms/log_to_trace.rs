use serde::{Deserialize, Serialize};

use crate::{
    config::{DataType, TransformConfig, TransformContext},
    event::Event,
    transforms::{FunctionTransform, Transform},
};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct LogToTrace;

#[async_trait::async_trait]
#[typetag::serde(name = "log_to_trace")]
impl TransformConfig for LogToTrace {
    async fn build(&self, _context: &TransformContext) -> crate::Result<Transform> {
        Ok(Transform::function(self.clone()))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn output_type(&self) -> DataType {
        DataType::Trace
    }

    fn transform_type(&self) -> &'static str {
        "log_to_trace"
    }
}

impl FunctionTransform for LogToTrace {
    fn transform(&mut self, output: &mut Vec<Event>, event: Event) {
        output.push(Event::Trace(event.into_log()));
    }
}
