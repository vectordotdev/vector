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
    #[derivative(Default(value = "0.9"))]
    pub(super) decrease_ratio: f64,

    // This value was picked as a reasonable default while we ensure the
    // viability of the system. This value may need adjustment if later
    // analysis discovers we need higher or lower weighting on past RTT
    // weighting.
    #[serde(default)]
    #[derivative(Default(value = "0.5"))]
    pub(super) ewma_alpha: f64,

    // This was picked as a reasonable default threshold ratio to avoid
    // dropping concurrency too aggressively when there is fluctuation
    // in the RTT measurements.
    #[serde(default)]
    #[derivative(Default(value = "0.05"))]
    pub(super) rtt_threshold_ratio: f64,
}
