#[cfg(test)]
mod noop;

#[cfg(test)]
pub use noop::NoopFramer;
use vector_core::transform::Transform;

#[typetag::serde(tag = "type")]
pub trait Framer: std::fmt::Debug + Send + Sync + dyn_clone::DynClone {
    fn name(&self) -> &'static str;

    fn build(&self) -> crate::Result<Transform<Vec<u8>>>;
}

dyn_clone::clone_trait_object!(Framer);

inventory::collect!(Box<dyn Framer>);
