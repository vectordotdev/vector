use super::Framer;
use serde::{Deserialize, Serialize};
use vector_core::transform::{FunctionTransform, Transform};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoopFramer;

#[typetag::serde(name = "noop")]
impl Framer for NoopFramer {
    fn name(&self) -> &'static str {
        "noop"
    }

    fn build(&self) -> crate::Result<Transform<Vec<u8>>> {
        Ok(Transform::function(NoopTransform))
    }
}

#[derive(Debug, Copy, Clone)]
struct NoopTransform;

impl FunctionTransform<Vec<u8>> for NoopTransform {
    fn transform(&mut self, output: &mut Vec<Vec<u8>>, input: Vec<u8>) {
        output.push(input)
    }
}

inventory::submit! {
    Box::new(NoopFramer) as Box<dyn Framer>
}
