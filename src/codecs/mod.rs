#[cfg(test)]
mod noop;

use crate::config::DataType;
#[cfg(test)]
pub use noop::NoopCodec;
use vector_core::{event::Event, transform::Transform};

#[typetag::serde(tag = "type")]
pub trait Codec: std::fmt::Debug + Send + Sync {
    fn name(&self) -> &'static str;

    fn build_decoder(&self) -> crate::Result<(Transform<Event>, DataType, DataType)>;

    fn build_encoder(&self) -> crate::Result<(Transform<Event>, DataType, DataType)>;
}

inventory::collect!(&'static dyn Codec);
