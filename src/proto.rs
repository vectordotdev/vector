#[cfg(any(feature = "sources-vector", feature = "sinks-vector"))]
use crate::event::proto as event;

#[cfg(any(feature = "sources-vector", feature = "sinks-vector"))]
pub mod vector;
