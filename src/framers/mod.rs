#[cfg(test)]
mod noop;

#[cfg(test)]
pub use noop::NoopFramer;
use vector_core::transform::Transform;

#[typetag::serde(tag = "type")]
pub trait Framer: std::fmt::Debug + Send + Sync {
    fn name(&self) -> &'static str;

    fn build(&self) -> crate::Result<Transform<Vec<u8>>>;
}

inventory::collect!(&'static dyn Framer);
