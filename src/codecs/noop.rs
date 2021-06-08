use super::Codec;
use crate::config::DataType;
use serde::{Deserialize, Serialize};
use vector_core::{
    event::Event,
    transform::{FunctionTransform, Transform},
};

#[derive(Debug, Serialize, Deserialize)]
pub struct NoopCodec;

#[typetag::serde(name = "noop")]
impl Codec for NoopCodec {
    fn name(&self) -> &'static str {
        "noop"
    }

    fn build_decoder(&self) -> crate::Result<(Transform<Event>, DataType, DataType)> {
        Ok((
            Transform::function(NoopTransform),
            DataType::Any,
            DataType::Any,
        ))
    }

    fn build_encoder(&self) -> crate::Result<(Transform<Event>, DataType, DataType)> {
        Ok((
            Transform::function(NoopTransform),
            DataType::Any,
            DataType::Any,
        ))
    }
}

#[derive(Copy, Clone)]
struct NoopTransform;

impl FunctionTransform<Event> for NoopTransform {
    fn transform(&mut self, output: &mut Vec<Event>, event: Event) {
        output.push(event)
    }
}

inventory::submit! {
    &NoopCodec as &dyn Codec
}
