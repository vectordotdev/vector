#[cfg(test)]
mod noop;

use crate::config::DataType;
#[cfg(test)]
pub use noop::NoopCodec;
use vector_core::{event::Event, transform::Transform};

#[typetag::serde(tag = "type")]
pub trait Codec: std::fmt::Debug + Send + Sync + dyn_clone::DynClone {
    fn name(&self) -> &'static str;

    fn build(&self, hint: CodecHint) -> crate::Result<CodecTransform>;
}

dyn_clone::clone_trait_object!(Codec);

pub enum CodecHint {
    Decoder,
    Encoder,
}

pub struct CodecTransform {
    pub input_type: DataType,
    pub transform: Transform<Event>,
}

inventory::collect!(Box<dyn Codec>);
