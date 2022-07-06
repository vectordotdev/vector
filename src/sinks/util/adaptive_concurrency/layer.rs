use tower::Layer;

use super::{AdaptiveConcurrencyLimit, AdaptiveConcurrencySettings};
use crate::sinks::util::retries::RetryLogic;

/// Enforces a limit on the concurrent number of requests the underlying
/// service can handle.
#[derive(Debug, Clone)]
pub struct AdaptiveConcurrencyLimitLayer<L> {
    concurrency: Option<usize>,
    options: AdaptiveConcurrencySettings,
    logic: L,
}

impl<L> AdaptiveConcurrencyLimitLayer<L> {
    /// Create a new concurrency limit layer.
    pub const fn new(
        concurrency: Option<usize>,
        options: AdaptiveConcurrencySettings,
        logic: L,
    ) -> Self {
        AdaptiveConcurrencyLimitLayer {
            concurrency,
            options,
            logic,
        }
    }
}

impl<S, L: RetryLogic> Layer<S> for AdaptiveConcurrencyLimitLayer<L> {
    type Service = AdaptiveConcurrencyLimit<S, L>;

    fn layer(&self, service: S) -> Self::Service {
        AdaptiveConcurrencyLimit::new(service, self.logic.clone(), self.concurrency, self.options)
    }
}
