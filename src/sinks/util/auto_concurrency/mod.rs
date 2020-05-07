//! Limit the max number of requests being concurrently processed.

pub mod future;
mod layer;
mod service;

pub(crate) use layer::AutoConcurrencyLimitLayer;
pub(crate) use service::AutoConcurrencyLimit;
