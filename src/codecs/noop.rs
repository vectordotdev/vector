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

    fn build(&self) -> crate::Result<Transform> {
        #[derive(Copy, Clone)]
        struct NoopTransform;

        impl FunctionTransform for NoopTransform {
            fn transform(&mut self, output: &mut Vec<Event>, event: Event) {
                output.push(event)
            }
        }

        Ok(Transform::function(NoopTransform))
    }
}
