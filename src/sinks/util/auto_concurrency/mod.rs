//! Limit the max number of requests being concurrently processed.

use serde::{Deserialize, Serialize};

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

#[derive(Clone, Copy, Debug, Derivative, Deserialize, Serialize)]
#[derivative(Default)]
pub struct AutoConcurrencySettings {
    #[serde(default)]
    #[derivative(Default(value = "0.5"))]
    pub(super) decrease_ratio: f64,
}
