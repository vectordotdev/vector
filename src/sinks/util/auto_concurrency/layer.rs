use super::AutoConcurrencyLimit;
use tower03::Layer;

/// Enforces a limit on the concurrent number of requests the underlying
/// service can handle.
#[derive(Debug, Clone)]
pub(crate) struct AutoConcurrencyLimitLayer {
    max: usize,
}

impl AutoConcurrencyLimitLayer {
    /// Create a new concurrency limit layer.
    pub fn new(max: usize) -> Self {
        AutoConcurrencyLimitLayer { max }
    }
}

impl<S> Layer<S> for AutoConcurrencyLimitLayer {
    type Service = AutoConcurrencyLimit<S>;

    fn layer(&self, service: S) -> Self::Service {
        AutoConcurrencyLimit::new(service, self.max)
    }
}
