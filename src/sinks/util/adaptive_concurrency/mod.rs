//! Limit the max number of requests being concurrently processed.

use serde::{Deserialize, Serialize};

mod controller;
mod future;
mod layer;
mod semaphore;
mod service;
mod tests;

pub(super) const MAX_CONCURRENCY: usize = 200;

pub(crate) use layer::AdaptiveConcurrencyLimitLayer;
pub(crate) use service::AdaptiveConcurrencyLimit;

pub(self) fn instant_now() -> std::time::Instant {
    tokio::time::Instant::now().into()
}

// The defaults for these values were chosen after running several
// simulations on a test service that had various responses to load. The
// values are the best balances found between competing outcomes.
#[derive(Clone, Copy, Debug, Derivative, Deserialize, Serialize)]
#[derivative(Default)]
#[serde(default, deny_unknown_fields)]
pub struct AdaptiveConcurrencySettings {
    // This value maintained high concurrency without holding it too
    // high under adverse conditions.
    #[derivative(Default(value = "0.9"))]
    pub(super) decrease_ratio: f64,

    // This value achieved the best balance between quick response and
    // stability.
    #[derivative(Default(value = "0.4"))]
    pub(super) ewma_alpha: f64,

    // This value avoided changing concurrency too aggressively when
    // there is fluctuation in the RTT measurements.
    #[derivative(Default(value = "2.5"))]
    pub(super) rtt_deviation_scale: f64,
}
