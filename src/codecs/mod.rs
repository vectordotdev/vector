#[cfg(test)]
mod noop;

#[cfg(test)]
pub use noop::NoopCodec;
use vector_core::{event::Event, transform::Transform};

#[typetag::serde(tag = "type")]
pub trait Codec: std::fmt::Debug + Send + Sync {
    fn name(&self) -> &'static str;

    fn build_decoder(&self) -> crate::Result<Transform<Event>>;

    fn build_encoder(&self) -> crate::Result<Transform<Event>>;
}

inventory::collect!(&'static dyn Codec);
