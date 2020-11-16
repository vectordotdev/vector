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

// The defaults for these values were chosen after running several
// simulations on a test service that had various responses to load. The
// values are the best balances found between competing outcomes.
#[derive(Clone, Copy, Debug, Derivative, Deserialize, Serialize)]
#[derivative(Default)]
pub struct AutoConcurrencySettings {
    // This value maintained high concurrency without holding it too
    // high under adverse conditions.
    #[serde(default)]
    #[derivative(Default(value = "0.9"))]
    pub(super) decrease_ratio: f64,

    // This value achieved the best balance between quick response and
    // stability.
    #[serde(default)]
    #[derivative(Default(value = "0.7"))]
    pub(super) ewma_alpha: f64,

    // This value avoided changing concurrency too aggressively when
    // there is fluctuation in the RTT measurements.
    #[serde(default)]
    #[derivative(Default(value = "0.05"))]
    pub(super) rtt_threshold_ratio: f64,
}
