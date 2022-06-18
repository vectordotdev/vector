use std::{
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    task::{Context, Poll},
};

use futures::{ready, FutureExt};
use futures_util::{future::BoxFuture, TryFuture};
use pin_project::pin_project;
use tokio::time::{sleep, Duration};
use tower::{load::Load, Service};

enum ServiceState {
    Healthcheck(BoxFuture<'static, bool>),
    Backoff(BoxFuture<'static, ()>),
    Ready,
}

/// A service which estimates load on inner service and
/// also forces backoff if the inner service is not healthy.
pub struct HealthService<S> {
    inner: S,
    healthcheck: Box<dyn Fn() -> BoxFuture<'static, bool> + Send>,
    reactivate_delay: Duration,
    /// Serves as an estimate of load and for notifying about errors.
    request_handle: Arc<AtomicBool>,
    state: ServiceState,
}

impl<S> HealthService<S> {
    pub fn new(
        inner: S,
        healthcheck: impl Fn() -> BoxFuture<'static, bool> + Send + 'static,
        reactivate_delay: std::time::Duration,
    ) -> Self {
        HealthService {
            inner,
            reactivate_delay: reactivate_delay.into(),
            request_handle: Arc::new(AtomicBool::new(false)),
            state: ServiceState::Healthcheck(healthcheck()),
            healthcheck: Box::new(healthcheck) as Box<_>,
        }
    }
}

impl<S, Req> Service<Req> for HealthService<S>
where
    S: Service<Req>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = LoadFuture<S::Future>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        loop {
            self.state = match self.state {
                ServiceState::Healthcheck(ref mut healthcheck) => {
                    if ready!(healthcheck.as_mut().poll(cx)) {
                        // Clear errors
                        self.request_handle.store(false, Ordering::Release);
                        ServiceState::Ready
                    } else {
                        ServiceState::Backoff(sleep(self.reactivate_delay).boxed())
                    }
                }
                ServiceState::Backoff(ref mut backoff) => {
                    ready!(backoff.as_mut().poll(cx));
                    ServiceState::Healthcheck((self.healthcheck)())
                }
                ServiceState::Ready => {
                    // Check for errors
                    if self.request_handle.load(Ordering::Acquire) {
                        // Check if the service is healthy
                        ServiceState::Healthcheck((self.healthcheck)())
                    } else {
                        return self.inner.poll_ready(cx);
                    }
                }
            }
        }
    }

    fn call(&mut self, req: Req) -> Self::Future {
        LoadFuture {
            inner: self.inner.call(req),
            request_handle: Arc::clone(&self.request_handle),
        }
    }
}

impl<S> Load for HealthService<S> {
    type Metric = usize;

    fn load(&self) -> Self::Metric {
        // The number of request handles is correlated to the number of requests
        // which is correlated with load.
        Arc::strong_count(&self.request_handle)
    }
}

/// Future for LoadService.
#[pin_project]
pub struct LoadFuture<F> {
    #[pin]
    inner: F,
    request_handle: Arc<AtomicBool>,
}

impl<F: TryFuture> Future for LoadFuture<F>
where
    F: Future<Output = Result<F::Ok, F::Error>>,
{
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // Poll inner
        let this = self.project();
        let output = ready!(this.inner.poll(cx));

        if output.is_err() {
            // Notify service of the error
            this.request_handle.store(true, Ordering::Release);
        }

        // Note: If we could extract status code here then
        // we could force backoff on StatusCode::TOO_MANY_REQUESTS
        // for this specific service.

        Poll::Ready(output)
    }
}
