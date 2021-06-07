#[cfg(test)]
mod noop;

#[cfg(test)]
pub use noop::NoopCodec;
use vector_core::transform::Transform;

#[typetag::serde(tag = "type")]
pub trait Codec: std::fmt::Debug + Send + Sync {
    fn name(&self) -> &'static str;

    fn build(&self) -> crate::Result<Transform>;
}

inventory::collect!(&'static dyn Codec);
