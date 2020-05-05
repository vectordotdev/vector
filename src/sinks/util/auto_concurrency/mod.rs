//! Limit the max number of requests being concurrently processed.

pub mod future;
mod layer;
mod service;

pub use self::{layer::AutoConcurrencyLimitLayer, service::AutoConcurrencyLimit};
