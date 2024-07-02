use std::{
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    task::{ready, Context, Poll},
};

use futures::FutureExt;
use futures_util::{future::BoxFuture, TryFuture};
use pin_project::pin_project;
use serde_with::serde_as;
use stream_cancel::{Trigger, Tripwire};
use tokio::time::{sleep, Duration};
use tower::Service;
use vector_lib::{configurable::configurable_component, emit};

use crate::{
    internal_events::{EndpointsActive, OpenGauge},
    sinks::util::retries::ExponentialBackoff,
};

const RETRY_MAX_DURATION_SECONDS_DEFAULT: u64 = 3_600;
const RETRY_INITIAL_BACKOFF_SECONDS_DEFAULT: u64 = 1;
const UNHEALTHY_AMOUNT_OF_ERRORS: usize = 5;

/// Options for determining the health of an endpoint.
#[serde_as]
#[configurable_component]
#[derive(Clone, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub struct HealthConfig {
    /// Initial delay between attempts to reactivate endpoints once they become unhealthy.
    #[serde(default = "default_retry_initial_backoff_secs")]
    #[configurable(metadata(docs::type_unit = "seconds"))]
    // not using Duration type because the value is only used as a u64.
    #[configurable(metadata(docs::human_name = "Retry Initial Backoff"))]
    pub retry_initial_backoff_secs: u64,

    /// Maximum delay between attempts to reactivate endpoints once they become unhealthy.
    #[serde_as(as = "serde_with::DurationSeconds<u64>")]
    #[serde(default = "default_retry_max_duration_secs")]
    #[configurable(metadata(docs::human_name = "Max Retry Duration"))]
    pub retry_max_duration_secs: Duration,
}

const fn default_retry_initial_backoff_secs() -> u64 {
    RETRY_INITIAL_BACKOFF_SECONDS_DEFAULT
}

const fn default_retry_max_duration_secs() -> std::time::Duration {
    Duration::from_secs(RETRY_MAX_DURATION_SECONDS_DEFAULT)
}

impl HealthConfig {
    pub fn build<S, L>(
        &self,
        logic: L,
        inner: S,
        open: OpenGauge,
        endpoint: String,
    ) -> HealthService<S, L> {
        let counters = Arc::new(HealthCounters::new());
        let snapshot = counters.snapshot();

        open.clone().open(emit_active_endpoints);
        HealthService {
            inner,
            logic,
            counters,
            snapshot,
            endpoint,
            state: CircuitState::Closed,
            open,
            // An exponential backoff starting from retry_initial_backoff_sec and doubling every time
            // up to retry_max_duration_secs.
            backoff: ExponentialBackoff::from_millis(2)
                .factor((self.retry_initial_backoff_secs.saturating_mul(1000) / 2).max(1))
                .max_delay(self.retry_max_duration_secs),
        }
    }
}

pub trait HealthLogic: Clone + Send + Sync + 'static {
    type Error: Send + Sync + 'static;
    type Response;

    /// Returns health of the endpoint based on the response/error.
    /// None if there is not enough information to determine it.
    fn is_healthy(&self, response: &Result<Self::Response, Self::Error>) -> Option<bool>;
}

enum CircuitState {
    /// Service is unhealthy hence it's not passing requests downstream.
    /// Contains timeout.
    Open(BoxFuture<'static, ()>),

    /// Service will pass one request to test its health.
    HalfOpen {
        permit: Option<Trigger>,
        done: Tripwire,
    },

    /// Service is healthy and passing requests downstream.
    Closed,
}

/// A service which monitors the health of a service.
/// Behaves like a circuit breaker.
pub struct HealthService<S, L> {
    inner: S,
    logic: L,
    counters: Arc<HealthCounters>,
    snapshot: HealthSnapshot,
    backoff: ExponentialBackoff,
    state: CircuitState,
    open: OpenGauge,
    endpoint: String,
}

impl<S, L, Req> Service<Req> for HealthService<S, L>
where
    L: HealthLogic<Response = S::Response, Error = S::Error>,
    S: Service<Req>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = HealthFuture<S::Future, L>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        loop {
            self.state = match self.state {
                CircuitState::Open(ref mut timer) => {
                    ready!(timer.as_mut().poll(cx));

                    debug!(message = "Endpoint is on probation.", endpoint = %&self.endpoint);

                    // Using Tripwire will let us be notified when the request is done.
                    // This can't be done through counters since a request can end without changing them.
                    let (permit, done) = Tripwire::new();

                    CircuitState::HalfOpen {
                        permit: Some(permit),
                        done,
                    }
                }
                CircuitState::HalfOpen {
                    permit: Some(_), ..
                } => {
                    // Pass one request to test health.
                    return self.inner.poll_ready(cx).map_err(Into::into);
                }
                CircuitState::HalfOpen {
                    permit: None,
                    ref mut done,
                } => {
                    let done = Pin::new(done);
                    ready!(done.poll(cx));

                    if self.counters.healthy(self.snapshot).is_ok() {
                        // A healthy response was observed
                        info!(message = "Endpoint is healthy.", endpoint = %&self.endpoint);

                        self.backoff.reset();
                        self.open.clone().open(emit_active_endpoints);
                        CircuitState::Closed
                    } else {
                        debug!(message = "Endpoint failed probation.", endpoint = %&self.endpoint);

                        CircuitState::Open(
                            sleep(self.backoff.next().expect("Should never end")).boxed(),
                        )
                    }
                }
                CircuitState::Closed => {
                    // Check for errors
                    match self.counters.healthy(self.snapshot) {
                        Ok(snapshot) => {
                            // Healthy
                            self.snapshot = snapshot;
                            return self.inner.poll_ready(cx).map_err(Into::into);
                        }
                        Err(errors) if errors >= UNHEALTHY_AMOUNT_OF_ERRORS => {
                            // Unhealthy
                            warn!(message = "Endpoint is unhealthy.", endpoint = %&self.endpoint);
                            CircuitState::Open(
                                sleep(self.backoff.next().expect("Should never end")).boxed(),
                            )
                        }
                        Err(_) => {
                            // Not ideal, but not enough errors to trip yet
                            return self.inner.poll_ready(cx).map_err(Into::into);
                        }
                    }
                }
            }
        }
    }

    fn call(&mut self, req: Req) -> Self::Future {
        let permit = if let CircuitState::HalfOpen { permit, .. } = &mut self.state {
            permit.take()
        } else {
            None
        };

        HealthFuture {
            inner: self.inner.call(req),
            logic: self.logic.clone(),
            counters: Arc::clone(&self.counters),
            permit,
        }
    }
}

/// Future for HealthService.
#[pin_project]
pub struct HealthFuture<F, L> {
    #[pin]
    inner: F,
    logic: L,
    counters: Arc<HealthCounters>,
    permit: Option<Trigger>,
}

impl<F: TryFuture, L> Future for HealthFuture<F, L>
where
    F: Future<Output = Result<F::Ok, F::Error>>,
    L: HealthLogic<Response = F::Ok, Error = F::Error>,
{
    type Output = Result<F::Ok, F::Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // Poll inner
        let this = self.project();
        let output = ready!(this.inner.poll(cx)).map_err(Into::into);

        match this.logic.is_healthy(&output) {
            None => (),
            Some(true) => this.counters.inc_healthy(),
            Some(false) => this.counters.inc_unhealthy(),
        }

        // Request is done so we can now drop the permit.
        this.permit.take();

        Poll::Ready(output)
    }
}

/// Tracker of response health, incremented by HealthFuture and used by HealthService.
struct HealthCounters {
    healthy: AtomicUsize,
    unhealthy: AtomicUsize,
}

impl HealthCounters {
    const fn new() -> Self {
        HealthCounters {
            healthy: AtomicUsize::new(0),
            unhealthy: AtomicUsize::new(0),
        }
    }

    fn inc_healthy(&self) {
        self.healthy.fetch_add(1, Ordering::Release);
    }

    fn inc_unhealthy(&self) {
        self.unhealthy.fetch_add(1, Ordering::Release);
    }

    /// Checks if healthy.
    ///
    /// Returns new snapshot if healthy.
    /// Else returns measure of unhealthy. Old snapshot is valid in that case.
    fn healthy(&self, snapshot: HealthSnapshot) -> Result<HealthSnapshot, usize> {
        let now = self.snapshot();

        // Compare current snapshot with given
        if now.healthy > snapshot.healthy {
            // Healthy response was observed
            Ok(now)
        } else if now.unhealthy > snapshot.unhealthy {
            // Unhealthy response was observed
            Err(now.unhealthy - snapshot.unhealthy)
        } else {
            // No relative observations
            Ok(now)
        }
    }

    fn snapshot(&self) -> HealthSnapshot {
        HealthSnapshot {
            healthy: self.healthy.load(Ordering::Acquire),
            unhealthy: self.unhealthy.load(Ordering::Acquire),
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq, Debug)]
struct HealthSnapshot {
    healthy: usize,
    unhealthy: usize,
}

fn emit_active_endpoints(count: usize) {
    emit!(EndpointsActive { count });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_counters() {
        let counters = HealthCounters::new();
        let mut snapshot = counters.snapshot();

        counters.inc_healthy();
        snapshot = counters.healthy(snapshot).unwrap();

        counters.inc_unhealthy();
        counters.inc_unhealthy();
        assert_eq!(counters.healthy(snapshot), Err(2));

        counters.inc_healthy();
        assert!(counters.healthy(snapshot).is_ok());
    }
}
