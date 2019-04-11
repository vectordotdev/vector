use crate::Error;
use futures::Poll;
use std::sync::{Arc, Mutex};
use tokio_executor::DefaultExecutor;
use tower_buffer::{future::ResponseFuture, Buffer, WorkerExecutor};
use tower_service::Service;

/// A buffer to lazily buffer requests
///
/// This buffer is a wrapper around `Buffer` which will lazily
/// spawn the background worker on the first call to `poll_ready`.
/// This allows one to create the buffer backed service without needing to
/// be within a futures context.
pub struct BufferLazy<T, Request, E>
where
    T: Service<Request>,
{
    inner: Arc<Mutex<State<T, Request>>>,
    buffer: Option<Buffer<T, Request>>,
    executor: E,
}

#[derive(Clone)]
enum State<T, Request>
where
    T: Service<Request>,
{
    Waiting(Option<T>, usize),
    Spawned(Buffer<T, Request>),
}

impl<T, Request> BufferLazy<T, Request, DefaultExecutor>
where
    T: Service<Request>,
{
    /// Create a new BufferLazy based on the provided service and bound
    pub fn new(svc: T, bound: usize) -> Self {
        BufferLazy {
            inner: Arc::new(Mutex::new(State::Waiting(Some(svc), bound))),
            buffer: None,
            executor: DefaultExecutor::current(),
        }
    }
}

impl<T, Request, E> BufferLazy<T, Request, E>
where
    T: Service<Request>,
    T::Error: Into<Error>,
    E: WorkerExecutor<T, Request> + Clone,
{
    /// Create a new BufferLazy based on the provided service and bound
    /// that will lazily spawn the worker on the provided executor.
    pub fn with_executor(svc: T, bound: usize, executor: E) -> Self {
        BufferLazy {
            inner: Arc::new(Mutex::new(State::Waiting(Some(svc), bound))),
            buffer: None,
            executor,
        }
    }
}

impl<T, Request, E> Service<Request> for BufferLazy<T, Request, E>
where
    T: Service<Request> + Send + 'static,
    T::Future: Send,
    T::Response: Send,
    T::Error: Into<Error> + Send + Sync,
    Request: Send + 'static,
    E: WorkerExecutor<T, Request> + Clone,
{
    type Response = T::Response;
    type Error = Error;
    type Future = ResponseFuture<T::Future>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        if let Some(buffer) = &mut self.buffer {
            buffer.poll_ready()
        } else {
            let mut inner = self.inner.lock().unwrap();
            match &mut *inner {
                State::Waiting(svc, bound) => {
                    let svc = svc.take().unwrap();
                    let mut buffer =
                        Buffer::with_executor(svc, *bound, &mut self.executor.clone())?;
                    *inner = State::Spawned(buffer.clone());
                    let poll_val = buffer.poll_ready();
                    self.buffer = Some(buffer);
                    poll_val
                }

                State::Spawned(buffer) => {
                    let mut buffer = buffer.clone();
                    let poll = buffer.poll_ready();
                    self.buffer = Some(buffer);
                    poll
                }
            }
        }
    }

    fn call(&mut self, request: Request) -> Self::Future {
        if let Some(buffer) = &mut self.buffer {
            buffer.call(request)
        } else {
            panic!(
                "This buffer has not spawned its background worker or you did not call poll_ready"
            );
        }
    }
}

impl<T, Request, E> Clone for BufferLazy<T, Request, E>
where
    T: Service<Request>,
    E: Clone,
{
    fn clone(&self) -> Self {
        BufferLazy {
            inner: self.inner.clone(),
            buffer: self.buffer.clone(),
            executor: self.executor.clone(),
        }
    }
}
