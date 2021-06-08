use super::Codec;
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

    fn build_decoder(&self) -> crate::Result<Transform> {
        Ok(Transform::function(NoopTransform))
    }

    fn build_encoder(&self) -> crate::Result<Transform> {
        Ok(Transform::function(NoopTransform))
    }
}

#[derive(Copy, Clone)]
struct NoopTransform;

impl FunctionTransform for NoopTransform {
    fn transform(&mut self, output: &mut Vec<Event>, event: Event) {
        output.push(event)
    }
}

inventory::submit! {
    &NoopCodec as &dyn Codec
}
