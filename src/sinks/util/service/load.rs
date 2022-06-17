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
use pin_project::{pin_project, pinned_drop};
use tokio::time::{sleep, Duration};
use tower::{load::Load, Service};

enum ServiceState {
    Healthcheck(BoxFuture<'static, bool>),
    Backoff(BoxFuture<'static, ()>),
    Ready,
}

/// A service which estimates load on it and also forces backoff if the service
/// is not healthy.
pub struct LoadService<S> {
    service: S,
    healthcheck: Box<dyn Fn() -> BoxFuture<'static, bool> + Send>,
    reactivate_delay: Duration,
    /// Serves as an estimate of load and for notifying about errors.
    request_handle: Arc<AtomicBool>,
    state: ServiceState,
}

impl<S> LoadService<S> {
    pub fn new(
        service: S,
        healthcheck: impl Fn() -> BoxFuture<'static, bool> + Send + 'static,
        reactivate_delay: std::time::Duration,
    ) -> Self {
        LoadService {
            service,
            reactivate_delay: reactivate_delay.into(),
            request_handle: Arc::new(AtomicBool::new(false)),
            state: ServiceState::Healthcheck(healthcheck()),
            healthcheck: Box::new(healthcheck) as Box<_>,
        }
    }
}

impl<S, Req> Service<Req> for LoadService<S>
where
    S: Service<Req>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = LoadFuture<S::Future>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        match &mut self.state {
            ServiceState::Healthcheck(healthcheck) => {
                if ready!(healthcheck.as_mut().poll(cx)) {
                    self.state = ServiceState::Ready;
                    // Clear errors
                    self.request_handle.store(false, Ordering::Release);
                    Poll::Ready(Ok(()))
                } else {
                    self.state = ServiceState::Backoff(sleep(self.reactivate_delay).boxed());
                    self.poll_ready(cx)
                }
            }
            ServiceState::Backoff(backoff) => {
                ready!(backoff.as_mut().poll(cx));
                self.state = ServiceState::Healthcheck((self.healthcheck)());
                self.poll_ready(cx)
            }
            ServiceState::Ready => {
                // Check for errors
                if self.request_handle.load(Ordering::Acquire) {
                    // Check if the service is healthy
                    self.state = ServiceState::Healthcheck((self.healthcheck)());
                    self.poll_ready(cx)
                } else {
                    Poll::Ready(Ok(()))
                }
            }
        }
    }

    fn call(&mut self, req: Req) -> Self::Future {
        LoadFuture {
            inner: self.service.call(req),
            request_handle: Some(Arc::clone(&self.request_handle)),
        }
    }
}

impl<S> Load for LoadService<S> {
    type Metric = usize;

    fn load(&self) -> Self::Metric {
        // The number of request handles is correlated to the number of requests
        // which is correlated with load.
        Arc::strong_count(&self.request_handle)
    }
}

#[pin_project(PinnedDrop)]
pub struct LoadFuture<F> {
    #[pin]
    inner: F,
    request_handle: Option<Arc<AtomicBool>>,
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

        let request_handle = this
            .request_handle
            .take()
            .expect("Poll called after completion.");
        if output.is_err() {
            // Notify service of the error
            request_handle.store(true, Ordering::Release);
        }

        // Note: If we could extract status code here then
        // we could force backoff on StatusCode::TOO_MANY_REQUESTS
        // for this specific service.

        Poll::Ready(output)
    }
}

#[pinned_drop]
impl<F> PinnedDrop for LoadFuture<F> {
    fn drop(self: Pin<&mut Self>) {
        if let Some(request_handle) = self.project().request_handle.take() {
            // Future dropped without completion. Better check its health.
            request_handle.store(true, Ordering::Release);
        }
    }
}
