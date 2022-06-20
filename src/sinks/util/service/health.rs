use std::{
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    task::{Context, Poll},
};

use futures::{ready, FutureExt};
use futures_util::{future::BoxFuture, TryFuture};
use pin_project::pin_project;
use tokio::time::{sleep, Duration};
use tower::{load::Load, Service};

use crate::sinks::util::retries::RetryLogic;

enum ServiceState {
    Healthcheck(BoxFuture<'static, bool>),
    Backoff {
        timer: BoxFuture<'static, ()>,
        healthcheck: bool,
    },
    Ready,
}

/// A service which estimates load on inner service and
/// also forces backoff if the inner service is not healthy.
pub struct HealthService<S, RL> {
    inner: S,
    healthcheck: Box<dyn Fn() -> BoxFuture<'static, bool> + Send>,
    logic: RL,
    reactivate_delay: Duration,
    /// Serves as an estimate of load and for notifying about errors.
    /// 0 - everything is fine
    /// 1 - check health
    /// 2 - backoff
    request_handle: Arc<AtomicUsize>,
    state: ServiceState,
}

impl<S, RL> HealthService<S, RL> {
    pub fn new(
        inner: S,
        healthcheck: impl Fn() -> BoxFuture<'static, bool> + Send + 'static,
        logic: RL,
        reactivate_delay: std::time::Duration,
    ) -> Self {
        HealthService {
            inner,
            reactivate_delay: reactivate_delay.into(),
            logic,
            request_handle: Arc::new(AtomicUsize::new(0)),
            state: ServiceState::Healthcheck(healthcheck()),
            healthcheck: Box::new(healthcheck) as Box<_>,
        }
    }
}

impl<S, RL, Req> Service<Req> for HealthService<S, RL>
where
    RL: RetryLogic<Response = S::Response>,
    S: Service<Req>,
    S::Error: Into<crate::Error>,
    // <S::Future as TryFuture>::Error: Into<crate::Error>,
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
                        self.request_handle.store(0, Ordering::Release);
                        ServiceState::Ready
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
                        self.request_handle.store(0, Ordering::Release);
                        ServiceState::Ready
                    }
                }
                ServiceState::Ready => {
                    // Check for errors
                    match self.request_handle.load(Ordering::Acquire) {
                        // No errors
                        0 => return self.inner.poll_ready(cx).map_err(Into::into),
                        // Check if the service is healthy
                        1 => ServiceState::Healthcheck((self.healthcheck)()),
                        // Backoff
                        2 => ServiceState::Backoff {
                            timer: sleep(self.reactivate_delay).boxed(),
                            healthcheck: false,
                        },
                        _ => unreachable!(),
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
    request_handle: Arc<AtomicUsize>,
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
                let _ = this.request_handle.compare_exchange_weak(
                    0,
                    2,
                    Ordering::Release,
                    Ordering::Relaxed,
                );
            }
            // Failure
            Some(false) => this.request_handle.store(1, Ordering::Release),
        }

        Poll::Ready(output)
    }
}
