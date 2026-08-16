use std::sync::Arc;

use tower::Layer;

use super::{
    AdaptiveConcurrencyLimit, AdaptiveConcurrencySettings, attempt::MeasureAttempt,
    controller::Controller,
};
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

/// Build a concurrency limit layer and the attempt reporter that feeds it, sharing one controller.
///
/// Apply the limiter outside the retry layer, so a whole retry sequence holds one permit, and the
/// reporter inside it, where one call is one attempt. The two only work as a pair: without the
/// reporter the controller never receives a round-trip time, so it never establishes a past average
/// and the limit stays at the initial concurrency for the life of the process.
pub(crate) fn measured_pair<L>(
    concurrency: Option<usize>,
    options: AdaptiveConcurrencySettings,
    logic: L,
) -> (MeasuredLimitLayer<L>, MeasureAttemptLayer<L>) {
    let controller = Arc::new(Controller::new(concurrency, options, logic));
    (
        MeasuredLimitLayer {
            controller: Arc::clone(&controller),
        },
        MeasureAttemptLayer { controller },
    )
}

/// The limiter half of [`measured_pair`].
pub(crate) struct MeasuredLimitLayer<L> {
    controller: Arc<Controller<L>>,
}

impl<S, L> Layer<S> for MeasuredLimitLayer<L> {
    type Service = AdaptiveConcurrencyLimit<S, L>;

    fn layer(&self, service: S) -> Self::Service {
        AdaptiveConcurrencyLimit::with_measured_attempts(service, Arc::clone(&self.controller))
    }
}

/// The reporter half of [`measured_pair`].
pub(crate) struct MeasureAttemptLayer<L> {
    controller: Arc<Controller<L>>,
}

impl<S, L> Layer<S> for MeasureAttemptLayer<L> {
    type Service = MeasureAttempt<S, L>;

    fn layer(&self, service: S) -> Self::Service {
        MeasureAttempt::new(service, Arc::clone(&self.controller))
    }
}
