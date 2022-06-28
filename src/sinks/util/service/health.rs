use std::{
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use crossbeam_utils::atomic::AtomicCell;
use futures::{ready, FutureExt};
use futures_util::{future::BoxFuture, TryFuture};
use pin_project::pin_project;
use tokio::time::{sleep, Duration};
use tower::{load::Load, Service};

use crate::{
    emit,
    internal_events::{EndpointsActive, OpenGauge, OpenToken},
    sinks::util::retries::RetryLogic,
};

enum ServiceState {
    Healthcheck(BoxFuture<'static, bool>),
    Backoff {
        timer: BoxFuture<'static, ()>,
        healthcheck: bool,
    },
    Ready(OpenToken<fn(usize)>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RequestError {
    Backpressure,
    Failure,
}

/// A service which estimates load on inner service and
/// also forces backoff if the inner service is not healthy.
pub struct HealthService<S, RL> {
    inner: S,
    healthcheck: Box<dyn Fn() -> BoxFuture<'static, bool> + Send>,
    logic: RL,
    reactivate_delay: Duration,
    /// Serves as an estimate of load and for notifying about errors.
    request_handle: Arc<AtomicCell<Option<RequestError>>>,
    state: ServiceState,
    open: OpenGauge,
}

impl<S, RL> HealthService<S, RL> {
    pub fn new(
        inner: S,
        healthcheck: impl Fn() -> BoxFuture<'static, bool> + Send + 'static,
        logic: RL,
        reactivate_delay: Duration,
        open: OpenGauge,
    ) -> Self {
        HealthService {
            inner,
            reactivate_delay,
            logic,
            request_handle: Arc::new(AtomicCell::new(None)),
            state: ServiceState::Healthcheck(healthcheck()),
            healthcheck: Box::new(healthcheck) as Box<_>,
            open,
        }
    }
}

impl<S, RL, Req> Service<Req> for HealthService<S, RL>
where
    RL: RetryLogic<Response = S::Response>,
    S: Service<Req>,
    S::Error: Into<crate::Error>,
{
    type Response = S::Response;
    type Error = crate::Error;
    type Future = HealthFuture<S::Future, RL>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        loop {
            self.state = match self.state {
                ServiceState::Healthcheck(ref mut healthcheck) => {
                    if ready!(healthcheck.as_mut().poll(cx)) {
                        // Clear errors
                        self.request_handle.store(None);
                        ServiceState::Ready(self.open.clone().open(emit_active_endpoints))
                    } else {
                        ServiceState::Backoff {
                            timer: sleep(self.reactivate_delay).boxed(),
                            healthcheck: true,
                        }
                    }
                }
                ServiceState::Backoff {
                    ref mut timer,
                    healthcheck,
                } => {
                    ready!(timer.as_mut().poll(cx));
                    if healthcheck {
                        ServiceState::Healthcheck((self.healthcheck)())
                    } else {
                        // Clear errors
                        self.request_handle.store(None);
                        ServiceState::Ready(self.open.clone().open(emit_active_endpoints))
                    }
                }
                ServiceState::Ready(_) => {
                    // Check for errors
                    match self.request_handle.load() {
                        // No errors
                        None => return self.inner.poll_ready(cx).map_err(Into::into),
                        // Backoff
                        Some(RequestError::Backpressure) => ServiceState::Backoff {
                            timer: sleep(self.reactivate_delay).boxed(),
                            healthcheck: false,
                        },
                        // Check if the service is healthy
                        Some(RequestError::Failure) => {
                            ServiceState::Healthcheck((self.healthcheck)())
                        }
                    }
                }
            }
        }
    }

    fn call(&mut self, req: Req) -> Self::Future {
        HealthFuture {
            inner: self.inner.call(req),
            logic: self.logic.clone(),
            request_handle: Arc::clone(&self.request_handle),
        }
    }
}

impl<S, RL> Load for HealthService<S, RL> {
    type Metric = usize;

    fn load(&self) -> Self::Metric {
        // The number of request handles is correlated to the number of requests
        // which is correlated with load.
        Arc::strong_count(&self.request_handle)
    }
}

/// Future for HealthService.
#[pin_project]
pub struct HealthFuture<F, RL> {
    #[pin]
    inner: F,
    logic: RL,
    request_handle: Arc<AtomicCell<Option<RequestError>>>,
}

impl<F: TryFuture, RL> Future for HealthFuture<F, RL>
where
    F: Future<Output = Result<F::Ok, F::Error>>,
    F::Error: Into<crate::Error>,
    RL: RetryLogic<Response = F::Ok>,
{
    type Output = Result<F::Ok, crate::Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // Poll inner
        let this = self.project();
        let output = ready!(this.inner.poll(cx)).map_err(Into::into);

        match this.logic.is_back_pressure(&output) {
            // Successful request
            None => (),
            // Backpressure
            Some(true) => {
                let _ = this
                    .request_handle
                    .compare_exchange(None, Some(RequestError::Backpressure));
            }
            // Failure
            Some(false) => this.request_handle.store(Some(RequestError::Failure)),
        }

        Poll::Ready(output)
    }
}

fn emit_active_endpoints(count: usize) {
    emit!(EndpointsActive { count });
}
