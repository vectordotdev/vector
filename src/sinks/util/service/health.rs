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
use tower::Service;
use vector_config::configurable_component;

use crate::{
    emit,
    internal_events::{EndpointsActive, OpenGauge, OpenToken},
    sinks::util::retries::ExponentialBackoff,
};

const RETRY_MAX_DURATION_SECONDS_DEFAULT: u64 = 3_600;
const RETRY_INITIAL_BACKOFF_SECONDS_DEFAULT: u64 = 1;
const UNHEALTHY_AMOUNT_OF_ERRORS: usize = 5;
/// How many errors is allowed in probation period before we consider the service unhealthy again.
/// Must be less than UNHEALTHY_AMOUNT_OF_ERRORS and greater than 0.
const PROBATION_AMOUNT: usize = 4;

/// Options for determining health of an endpoint.
#[configurable_component]
#[derive(Clone, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub struct HealthConfig {
    /// Initial timeout, in seconds, between attempts to reactivate endpoints once they become unhealthy.
    pub retry_initial_backoff_secs: Option<u64>,

    /// Maximum timeout, in seconds, between attempts to reactivate endpoints once they become unhealthy.
    pub retry_max_duration_secs: Option<u64>,
}

impl HealthConfig {
    pub fn build<S, L>(
        &self,
        logic: L,
        inner: S,
        healthcheck: impl Fn() -> BoxFuture<'static, bool> + Send + 'static,
        open: OpenGauge,
    ) -> HealthService<S, L> {
        let counters = Arc::new(HealthCounters::new());
        let snapshot = counters.snapshot();

        HealthService {
            inner,
            logic,
            counters,
            snapshot,
            open,
            state: ServiceState::Healthcheck(healthcheck()),
            healthcheck: Box::new(healthcheck) as Box<_>,
            // An exponential backoff starting from retry_initial_backoff_sec and doubling every time
            // up to retry_max_duration_secs.
            backoff: ExponentialBackoff::from_millis(2)
                .factor(
                    (self
                        .retry_initial_backoff_secs
                        .unwrap_or(RETRY_INITIAL_BACKOFF_SECONDS_DEFAULT)
                        .saturating_mul(1000)
                        / 2)
                    .max(1),
                )
                .max_delay(Duration::from_secs(
                    self.retry_max_duration_secs
                        .unwrap_or(RETRY_MAX_DURATION_SECONDS_DEFAULT),
                )),
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

enum ServiceState {
    /// Service is unhealthy and should be checked once timeout expires.
    Unhealthy(BoxFuture<'static, ()>),
    /// Healthcheck in progress.
    Healthcheck(BoxFuture<'static, bool>),
    /// Service is healthy.
    Healthy(OpenToken<fn(usize)>),
}

/// A service which monitors the health of a service.
pub struct HealthService<S, L> {
    inner: S,
    healthcheck: Box<dyn Fn() -> BoxFuture<'static, bool> + Send>,
    logic: L,
    counters: Arc<HealthCounters>,
    snapshot: HealthSnapshot,
    backoff: ExponentialBackoff,
    state: ServiceState,
    open: OpenGauge,
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
                ServiceState::Unhealthy(ref mut timer) => {
                    ready!(timer.as_mut().poll(cx));
                    // It needs to at least pass healthcheck before it can be healthy again
                    ServiceState::Healthcheck((self.healthcheck)())
                }
                ServiceState::Healthcheck(ref mut healthcheck) => {
                    if ready!(healthcheck.as_mut().poll(cx)) {
                        // It's at least reachable so let's try again
                        self.snapshot = self.counters.probation(
                            self.snapshot,
                            UNHEALTHY_AMOUNT_OF_ERRORS - PROBATION_AMOUNT,
                        );
                        debug!("Service is maybe healthy.");
                        ServiceState::Healthy(self.open.clone().open(emit_active_endpoints))
                    } else {
                        ServiceState::Unhealthy(
                            sleep(self.backoff.next().expect("Should never end")).boxed(),
                        )
                    }
                }
                ServiceState::Healthy(_) => {
                    // Check for errors
                    match self.counters.healthy(self.snapshot) {
                        Ok(snapshot) => {
                            // Healthy
                            self.snapshot = snapshot;
                            self.backoff.reset();
                            return self.inner.poll_ready(cx).map_err(Into::into);
                        }
                        Err(errors) if errors >= UNHEALTHY_AMOUNT_OF_ERRORS => {
                            // Unhealthy
                            debug!("Service is unhealthy.");
                            ServiceState::Unhealthy(
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
        HealthFuture {
            inner: self.inner.call(req),
            logic: self.logic.clone(),
            counters: Arc::clone(&self.counters),
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

    /// Returns snapshot with given amount of unhealthy that will fail on healthy, for amount > 0,
    /// until at least one healthy response has been received since snapshot.
    fn probation(&self, snapshot: HealthSnapshot, amount: usize) -> HealthSnapshot {
        HealthSnapshot {
            // Leave healthy counter as is to detect any healthy response
            healthy: snapshot.healthy,
            // Set unhealthy diff to amount
            unhealthy: self
                .unhealthy
                .load(Ordering::Acquire)
                .saturating_sub(amount),
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

    #[test]
    fn test_counters_probation() {
        let counters = HealthCounters::new();
        let mut snapshot = counters.snapshot();

        counters.inc_unhealthy();
        counters.inc_unhealthy();
        snapshot = counters.probation(snapshot, 1);
        assert_eq!(counters.healthy(snapshot), Err(1));

        counters.inc_unhealthy();
        assert!(counters.healthy(snapshot).is_err());

        counters.inc_healthy();
        assert!(counters.healthy(snapshot).is_ok());
    }

    #[test]
    fn test_counters_obsolete_probation() {
        let counters = HealthCounters::new();
        let mut snapshot = counters.snapshot();

        counters.inc_unhealthy();
        counters.inc_unhealthy();
        assert!(counters.healthy(snapshot).is_err());

        counters.inc_healthy();
        snapshot = counters.probation(snapshot, 1);
        assert!(counters.healthy(snapshot).is_ok());
    }
}
