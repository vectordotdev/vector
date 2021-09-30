use serde::{Deserialize, Serialize};

use crate::{
    config::{DataType, Output, TransformConfig, TransformContext},
    event::Event,
    transforms::{FunctionTransform, Transform},
};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct TraceToLog;

#[async_trait::async_trait]
#[typetag::serde(name = "trace_to_log")]
impl TransformConfig for TraceToLog {
    async fn build(&self, _context: &TransformContext) -> crate::Result<Transform> {
        Ok(Transform::function(self.clone()))
    }

    fn input_type(&self) -> DataType {
        DataType::Trace
    }

    fn outputs(&self) -> Vec<Output> {
        vec![Output::default(DataType::Log)]
    }

    fn transform_type(&self) -> &'static str {
        "trace_to_log"
    }
}

impl FunctionTransform for TraceToLog {
    fn transform(&mut self, output: &mut Vec<Event>, event: Event) {
        output.push(Event::Log(event.into_log()));
    }
}
