mod layer;
mod service;

pub(crate) type Error = Box<std::error::Error + Send + Sync + 'static>;

pub use crate::layer::BufferLazyLayer;
pub use crate::service::BufferLazy;
