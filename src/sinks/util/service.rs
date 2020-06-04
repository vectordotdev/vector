use super::{
    retries::{FixedRetryPolicy, RetryLogic},
    sink::Response,
    Batch, BatchSettings, BatchSink,
};
use crate::buffers::Acker;
use futures01::{Async, Future, Poll};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::{error, fmt};
use tokio01::timer::Delay;
use tower::{
    layer::{util::Stack, Layer},
    limit::{concurrency::ConcurrencyLimit, rate::RateLimit},
    retry::Retry,
    util::BoxService,
    Service, ServiceBuilder,
};

pub type TowerBatchedSink<S, B, L, Request> =
    BatchSink<ConcurrencyLimit<RateLimit<Retry<FixedRetryPolicy<L>, Timeout<S>>>>, B, Request>;

pub trait ServiceBuilderExt<L> {
    fn map<R1, R2, F>(self, f: F) -> ServiceBuilder<Stack<MapLayer<R1, R2>, L>>
    where
        F: Fn(R1) -> R2 + Send + Sync + 'static;

    fn settings<RL, Request>(
        self,
        settings: TowerRequestSettings,
        retry_logic: RL,
    ) -> ServiceBuilder<Stack<TowerRequestLayer<RL, Request>, L>>;
}

impl<L> ServiceBuilderExt<L> for ServiceBuilder<L> {
    fn map<R1, R2, F>(self, f: F) -> ServiceBuilder<Stack<MapLayer<R1, R2>, L>>
    where
        F: Fn(R1) -> R2 + Send + Sync + 'static,
    {
        self.layer(MapLayer { f: Arc::new(f) })
    }

    fn settings<RL, Request>(
        self,
        settings: TowerRequestSettings,
        retry_logic: RL,
    ) -> ServiceBuilder<Stack<TowerRequestLayer<RL, Request>, L>> {
        self.layer(TowerRequestLayer {
            settings,
            retry_logic,
            _pd: std::marker::PhantomData,
        })
    }
}

/// Tower Request based configuration
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
pub struct TowerRequestConfig {
    pub in_flight_limit: Option<usize>,        // 5
    pub timeout_secs: Option<u64>,             // 60
    pub rate_limit_duration_secs: Option<u64>, // 1
    pub rate_limit_num: Option<u64>,           // 5
    pub retry_attempts: Option<usize>,         // max_value()
    pub retry_max_duration_secs: Option<u64>,
    pub retry_initial_backoff_secs: Option<u64>, // 1
}

impl TowerRequestConfig {
    pub fn unwrap_with(&self, defaults: &TowerRequestConfig) -> TowerRequestSettings {
        TowerRequestSettings {
            in_flight_limit: self
                .in_flight_limit
                .or(defaults.in_flight_limit)
                .unwrap_or(5),
            timeout: Duration::from_secs(self.timeout_secs.or(defaults.timeout_secs).unwrap_or(60)),
            rate_limit_duration: Duration::from_secs(
                self.rate_limit_duration_secs
                    .or(defaults.rate_limit_duration_secs)
                    .unwrap_or(1),
            ),
            rate_limit_num: self.rate_limit_num.or(defaults.rate_limit_num).unwrap_or(5),
            retry_attempts: self
                .retry_attempts
                .or(defaults.retry_attempts)
                .unwrap_or(usize::max_value()),
            retry_max_duration_secs: Duration::from_secs(
                self.retry_max_duration_secs
                    .or(defaults.retry_max_duration_secs)
                    .unwrap_or(3600),
            ),
            retry_initial_backoff_secs: Duration::from_secs(
                self.retry_initial_backoff_secs
                    .or(defaults.retry_initial_backoff_secs)
                    .unwrap_or(1),
            ),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TowerRequestSettings {
    pub in_flight_limit: usize,
    pub timeout: Duration,
    pub rate_limit_duration: Duration,
    pub rate_limit_num: u64,
    pub retry_attempts: usize,
    pub retry_max_duration_secs: Duration,
    pub retry_initial_backoff_secs: Duration,
}

impl TowerRequestSettings {
    pub fn retry_policy<L: RetryLogic>(&self, logic: L) -> FixedRetryPolicy<L> {
        FixedRetryPolicy::new(
            self.retry_attempts,
            self.retry_initial_backoff_secs,
            self.retry_max_duration_secs,
            logic,
        )
    }

    pub fn batch_sink<B, L, S, Request>(
        &self,
        retry_logic: L,
        service: S,
        batch: B,
        batch_settings: BatchSettings,
        acker: Acker,
    ) -> TowerBatchedSink<S, B, L, Request>
    // Would like to return `impl Sink + SinkExt<T>` here, but that
    // doesn't work with later calls to `batched_with_min` etc (via
    // `trait SinkExt` above), as it is missing a bound on the
    // associated types that cannot be expressed in stable Rust.
    where
        L: RetryLogic<Response = S::Response> + Send + 'static,
        S: Service<Request> + Clone + Send + 'static,
        S::Error: Into<crate::Error> + Send + Sync + 'static,
        S::Response: Send + Response,
        S::Future: Send + 'static,
        B: Batch<Output = Request>,
        Request: Send + Clone + 'static,
    {
        let policy = self.retry_policy(retry_logic);
        let service = ServiceBuilder::new()
            .concurrency_limit(self.in_flight_limit)
            .rate_limit(self.rate_limit_num, self.rate_limit_duration)
            .retry(policy)
            .layer(TimeoutLayer {
                timeout: self.timeout,
            })
            .service(service);

        BatchSink::new(service, batch, batch_settings, acker)
    }
}

#[derive(Debug, Clone)]
pub struct TowerRequestLayer<L, Request> {
    settings: TowerRequestSettings,
    retry_logic: L,
    _pd: std::marker::PhantomData<Request>,
}

impl<S, L, Request> tower::layer::Layer<S> for TowerRequestLayer<L, Request>
where
    S: Service<Request> + Send + Clone + 'static,
    S::Response: Response + Send + 'static,
    S::Error: Into<crate::Error> + Send + Sync + 'static,
    S::Future: Send + 'static,
    L: RetryLogic<Response = S::Response> + Send + 'static,
    Request: Clone + Send + 'static,
{
    type Service = BoxService<Request, S::Response, crate::Error>;

    fn layer(&self, inner: S) -> Self::Service {
        let policy = self.settings.retry_policy(self.retry_logic.clone());

        let l = ServiceBuilder::new()
            .concurrency_limit(self.settings.in_flight_limit)
            .rate_limit(
                self.settings.rate_limit_num,
                self.settings.rate_limit_duration,
            )
            .retry(policy)
            .layer(TimeoutLayer {
                timeout: self.settings.timeout,
            })
            .service(inner);

        BoxService::new(l)
    }
}

// === map ===

pub struct MapLayer<R1, R2> {
    f: Arc<dyn Fn(R1) -> R2 + Send + Sync + 'static>,
}

impl<S, R1, R2> Layer<S> for MapLayer<R1, R2>
where
    S: Service<R2>,
{
    type Service = Map<S, R1, R2>;

    fn layer(&self, inner: S) -> Self::Service {
        Map {
            f: self.f.clone(),
            inner,
        }
    }
}

pub struct Map<S, R1, R2> {
    f: Arc<dyn Fn(R1) -> R2 + Send + Sync + 'static>,
    inner: S,
}

impl<S, R1, R2> Service<R1> for Map<S, R1, R2>
where
    S: Service<R2>,
    crate::Error: From<S::Error>,
{
    type Response = S::Response;
    type Error = crate::Error;
    type Future = futures01::future::MapErr<S::Future, fn(S::Error) -> crate::Error>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.inner.poll_ready().map_err(Into::into)
    }

    fn call(&mut self, req: R1) -> Self::Future {
        let req = (self.f)(req);
        self.inner.call(req).map_err(|e| e.into())
    }
}

impl<S: Clone, R1, R2> Clone for Map<S, R1, R2> {
    fn clone(&self) -> Self {
        Self {
            f: self.f.clone(),
            inner: self.inner.clone(),
        }
    }
}

// === timeout ===

/// Applies a timeout to requests.
///
/// We require our own timeout layer because the current
/// 0.1 version uses From intead of Into bounds for errors
/// this casues the whole stack to not align and not compile.
/// In future versions of tower this should be fixed.
#[derive(Debug, Clone)]
pub struct Timeout<T> {
    inner: T,
    timeout: Duration,
}

// ===== impl Timeout =====

impl<S, Request> Service<Request> for Timeout<S>
where
    S: Service<Request>,
    S::Error: Into<crate::Error>,
{
    type Response = S::Response;
    type Error = crate::Error;
    type Future = ResponseFuture<S::Future>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.inner.poll_ready().map_err(Into::into)
    }

    fn call(&mut self, request: Request) -> Self::Future {
        let response = self.inner.call(request);
        let sleep = Delay::new(Instant::now() + self.timeout);

        ResponseFuture { response, sleep }
    }
}

/// Applies a timeout to requests via the supplied inner service.
#[derive(Debug)]
pub struct TimeoutLayer {
    timeout: Duration,
}

impl TimeoutLayer {
    /// Create a timeout from a duration
    pub fn new(timeout: Duration) -> Self {
        TimeoutLayer { timeout }
    }
}

impl<S> Layer<S> for TimeoutLayer {
    type Service = Timeout<S>;

    fn layer(&self, service: S) -> Self::Service {
        Timeout {
            inner: service,
            timeout: self.timeout,
        }
    }
}

/// `Timeout` response future
#[derive(Debug)]
pub struct ResponseFuture<T> {
    response: T,
    sleep: Delay,
}

impl<T> Future for ResponseFuture<T>
where
    T: Future,
    T::Error: Into<crate::Error>,
{
    type Item = T::Item;
    type Error = crate::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        // First, try polling the future
        match self.response.poll().map_err(Into::into)? {
            Async::Ready(v) => return Ok(Async::Ready(v)),
            Async::NotReady => {}
        }

        // Now check the sleep
        match self.sleep.poll()? {
            Async::NotReady => Ok(Async::NotReady),
            Async::Ready(_) => Err(Elapsed(()).into()),
        }
    }
}

#[derive(Debug)]
pub struct Elapsed(pub(super) ());

impl Elapsed {
    /// Construct a new elapsed error
    pub fn new() -> Self {
        Elapsed(())
    }
}

impl fmt::Display for Elapsed {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.pad("request timed out")
    }
}

impl error::Error for Elapsed {}

// rustc issue: https://github.com/rust-lang/rust/issues/71259
#[cfg(feature = "disabled")]
#[cfg(test)]
mod tests {
    use super::*;
    use futures01::Future;
    use std::sync::Arc;
    use tokio01_test::{assert_ready, task::MockTask};
    use tower::layer::Layer;
    use tower_test::{assert_request_eq, mock};

    #[test]
    fn map() {
        let mut task = MockTask::new();
        let (mock, mut handle) = mock::pair();

        let f = |r| r;

        let map_layer = MapLayer { f: Arc::new(f) };

        let mut svc = map_layer.layer(mock);

        task.enter(|| assert_ready!(svc.poll_ready()));

        let res = svc.call("hello world");

        assert_request_eq!(handle, "hello world").send_response("world bye");

        res.wait().unwrap();
    }
}
