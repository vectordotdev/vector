#[cfg(any(feature = "sources-vector_grpc", feature = "sinks-vector_grpc"))]
use crate::event::proto as event;

#[cfg(any(feature = "sources-vector_grpc", feature = "sinks-vector_grpc"))]
pub(crate) mod vector;
