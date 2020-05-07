use super::future::ResponseFuture;

use tower03::Service;

use futures::ready;
use std::fmt;
use std::future::Future;
use std::mem;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

/// Enforces a limit on the concurrent number of requests the underlying
/// service can handle.
#[derive(Debug)]
pub struct AutoConcurrencyLimit<T> {
    inner: T,
    semaphore: Arc<Semaphore>,
    state: State,
}

enum State {
    Waiting(Pin<Box<dyn Future<Output = OwnedSemaphorePermit> + Send + 'static>>),
    Ready(OwnedSemaphorePermit),
    Empty,
}

impl<T> AutoConcurrencyLimit<T> {
    /// Create a new concurrency limiter.
    pub(crate) fn new(inner: T, max: usize) -> Self {
        AutoConcurrencyLimit {
            inner,
            semaphore: Arc::new(Semaphore::new(max)),
            state: State::Empty,
        }
    }
}

impl<S, Request> Service<Request> for AutoConcurrencyLimit<S>
where
    S: Service<Request>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = ResponseFuture<S::Future>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        loop {
            self.state = match self.state {
                State::Ready(_) => return self.inner.poll_ready(cx),
                State::Waiting(ref mut fut) => {
                    tokio::pin!(fut);
                    let permit = ready!(fut.poll(cx));
                    State::Ready(permit)
                }
                State::Empty => State::Waiting(Box::pin(self.semaphore.clone().acquire_owned())),
            };
        }
    }

    fn call(&mut self, request: Request) -> Self::Future {
        // Make sure a permit has been acquired
        let permit = match mem::replace(&mut self.state, State::Empty) {
            // Take the permit.
            State::Ready(permit) => permit,
            // whoopsie!
            _ => panic!("max requests in-flight; poll_ready must be called first"),
        };

        // Call the inner service
        let future = self.inner.call(request);

        ResponseFuture::new(future, permit)
    }
}

impl<S> Clone for AutoConcurrencyLimit<S>
where
    S: Clone,
{
    fn clone(&self) -> AutoConcurrencyLimit<S> {
        AutoConcurrencyLimit {
            inner: self.inner.clone(),
            semaphore: self.semaphore.clone(),
            state: State::Empty,
        }
    }
}

impl fmt::Debug for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            State::Waiting(_) => f
                .debug_tuple("State::Waiting")
                .field(&format_args!("..."))
                .finish(),
            State::Ready(ref r) => f.debug_tuple("State::Ready").field(&r).finish(),
            State::Empty => f.debug_tuple("State::Empty").finish(),
        }
    }
}
