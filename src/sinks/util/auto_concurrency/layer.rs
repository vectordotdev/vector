use super::AutoConcurrencyLimit;
use crate::sinks::util::retries2::RetryLogic;
use tower03::Layer;

/// Enforces a limit on the concurrent number of requests the underlying
/// service can handle.
#[derive(Debug, Clone)]
pub(crate) struct AutoConcurrencyLimitLayer<L> {
    max: usize,
    logic: L,
}

impl<L> AutoConcurrencyLimitLayer<L> {
    /// Create a new concurrency limit layer.
    pub fn new(max: usize, logic: L) -> Self {
        AutoConcurrencyLimitLayer { max, logic }
    }
}

impl<S, L: RetryLogic> Layer<S> for AutoConcurrencyLimitLayer<L> {
    type Service = AutoConcurrencyLimit<S, L>;

    fn layer(&self, service: S) -> Self::Service {
        AutoConcurrencyLimit::new(service, self.logic.clone(), self.max)
    }
}
