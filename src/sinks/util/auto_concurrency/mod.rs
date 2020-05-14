//! Limit the max number of requests being concurrently processed.

mod controller;
mod future;
mod layer;
mod semaphore;
mod service;

pub(crate) use layer::AutoConcurrencyLimitLayer;
pub(crate) use service::AutoConcurrencyLimit;
