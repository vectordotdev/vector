//! Limit the max number of requests being concurrently processed.

mod controller;
mod future;
mod layer;
mod semaphore;
mod service;
mod tests;

pub(super) const MAX_CONCURRENCY: usize = 200;

pub(crate) use layer::AutoConcurrencyLimitLayer;
pub(crate) use service::AutoConcurrencyLimit;

pub(self) fn instant_now() -> std::time::Instant {
    tokio::time::Instant::now().into()
}
