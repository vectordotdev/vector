use crate::{BufferLazy, Error};
use tokio_executor::DefaultExecutor;
use tower_buffer::WorkerExecutor;
use tower_layer::Layer;
use tower_service::Service;

/// BufferLazy requests with a bounded buffer
///
/// This will use the `DefaultExecutor` or the provided executor
/// to lazily spawn the background worker on the first call to
/// `poll_ready`.
pub struct BufferLazyLayer<E = DefaultExecutor> {
    bound: usize,
    executor: E,
}

impl BufferLazyLayer<DefaultExecutor> {
    pub fn new(bound: usize) -> Self {
        BufferLazyLayer {
            bound,
            executor: DefaultExecutor::current(),
        }
    }
}

impl<E> BufferLazyLayer<E> {
    pub fn with_executor<S, Request>(bound: usize, executor: E) -> Self
    where
        S: Service<Request>,
        S::Error: Into<Error>,
        E: WorkerExecutor<S, Request> + Clone,
    {
        BufferLazyLayer { bound, executor }
    }
}

impl<E, S, Request> Layer<S, Request> for BufferLazyLayer<E>
where
    S: Service<Request> + Send + 'static,
    S::Future: Send,
    S::Response: Send,
    S::Error: Into<Error> + Send + Sync,
    Request: Send + 'static,
    E: WorkerExecutor<S, Request> + Clone,
{
    type Response = S::Response;
    type Error = Error;
    type LayerError = Error;
    type Service = BufferLazy<S, Request, E>;

    fn layer(&self, service: S) -> Result<Self::Service, Self::LayerError> {
        Ok(BufferLazy::with_executor(
            service,
            self.bound,
            self.executor.clone(),
        ))
    }
}
