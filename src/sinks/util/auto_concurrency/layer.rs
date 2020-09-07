use super::{AutoConcurrencyLimit, AutoConcurrencySettings};
use crate::sinks::util::retries::RetryLogic;
use tower::Layer;

/// Enforces a limit on the concurrent number of requests the underlying
/// service can handle.
#[derive(Debug, Clone)]
pub(crate) struct AutoConcurrencyLimitLayer<L> {
    in_flight_limit: Option<usize>,
    options: AutoConcurrencySettings,
    logic: L,
}

impl<L> AutoConcurrencyLimitLayer<L> {
    /// Create a new concurrency limit layer.
    pub fn new(in_flight_limit: Option<usize>, options: AutoConcurrencySettings, logic: L) -> Self {
        AutoConcurrencyLimitLayer {
            in_flight_limit,
            options,
            logic,
        }
    }
}

impl<S, L: RetryLogic> Layer<S> for AutoConcurrencyLimitLayer<L> {
    type Service = AutoConcurrencyLimit<S, L>;

    fn layer(&self, service: S) -> Self::Service {
        AutoConcurrencyLimit::new(
            service,
            self.logic.clone(),
            self.in_flight_limit,
            self.options,
        )
    }
}
