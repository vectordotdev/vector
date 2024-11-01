//! Limit the max number of requests being concurrently processed.

mod controller;
mod future;
mod layer;
mod semaphore;
mod service;

#[cfg(test)]
pub mod tests;

pub(crate) use layer::AdaptiveConcurrencyLimitLayer;
pub(crate) use service::AdaptiveConcurrencyLimit;
use vector_lib::configurable::configurable_component;

fn instant_now() -> std::time::Instant {
    tokio::time::Instant::now().into()
}

/// Configuration of adaptive concurrency parameters.
///
/// These parameters typically do not require changes from the default, and incorrect values can lead to meta-stable or
/// unstable performance and sink behavior. Proceed with caution.
// The defaults for these values were chosen after running several simulations on a test service that had
// various responses to load. The values are the best balances found between competing outcomes.
#[configurable_component]
#[derive(Clone, Copy, Debug)]
#[serde(deny_unknown_fields)]
pub struct AdaptiveConcurrencySettings {
    /// The initial concurrency limit to use. If not specified, the initial limit will be 1 (no concurrency).
    ///
    /// It is recommended to set this value to your service's average limit if you're seeing that it takes a
    /// long time to ramp up adaptive concurrency after a restart. You can find this value by looking at the
    /// `adaptive_concurrency_limit` metric.
    #[configurable(validation(range(min = 1)))]
    #[serde(default = "default_initial_concurrency")]
    pub(super) initial_concurrency: usize,

    /// The fraction of the current value to set the new concurrency limit when decreasing the limit.
    ///
    /// Valid values are greater than `0` and less than `1`. Smaller values cause the algorithm to scale back rapidly
    /// when latency increases.
    ///
    /// Note that the new limit is rounded down after applying this ratio.
    #[configurable(validation(range(min = 0.0, max = 1.0)))]
    #[serde(default = "default_decrease_ratio")]
    pub(super) decrease_ratio: f64,

    /// The weighting of new measurements compared to older measurements.
    ///
    /// Valid values are greater than `0` and less than `1`.
    ///
    /// ARC uses an exponentially weighted moving average (EWMA) of past RTT measurements as a reference to compare with
    /// the current RTT. Smaller values cause this reference to adjust more slowly, which may be useful if a service has
    /// unusually high response variability.
    #[configurable(validation(range(min = 0.0, max = 1.0)))]
    #[serde(default = "default_ewma_alpha")]
    pub(super) ewma_alpha: f64,

    /// Scale of RTT deviations which are not considered anomalous.
    ///
    /// Valid values are greater than or equal to `0`, and we expect reasonable values to range from `1.0` to `3.0`.
    ///
    /// When calculating the past RTT average, we also compute a secondary “deviation” value that indicates how variable
    /// those values are. We use that deviation when comparing the past RTT average to the current measurements, so we
    /// can ignore increases in RTT that are within an expected range. This factor is used to scale up the deviation to
    /// an appropriate range.  Larger values cause the algorithm to ignore larger increases in the RTT.
    #[configurable(validation(range(min = 0.0)))]
    #[serde(default = "default_rtt_deviation_scale")]
    pub(super) rtt_deviation_scale: f64,

    /// The maximum concurrency limit.
    ///
    /// The adaptive request concurrency limit will not go above this bound. This is put in place as a safeguard.
    #[configurable(validation(range(min = 1)))]
    #[serde(default = "default_max_concurrency_limit")]
    pub(super) max_concurrency_limit: usize,
}

const fn default_initial_concurrency() -> usize {
    1
}

const fn default_decrease_ratio() -> f64 {
    0.9
}

const fn default_ewma_alpha() -> f64 {
    0.4
}

const fn default_rtt_deviation_scale() -> f64 {
    2.5
}

const fn default_max_concurrency_limit() -> usize {
    200
}

impl Default for AdaptiveConcurrencySettings {
    fn default() -> Self {
        Self {
            initial_concurrency: default_initial_concurrency(),
            decrease_ratio: default_decrease_ratio(),
            ewma_alpha: default_ewma_alpha(),
            rtt_deviation_scale: default_rtt_deviation_scale(),
            max_concurrency_limit: default_max_concurrency_limit(),
        }
    }
}
