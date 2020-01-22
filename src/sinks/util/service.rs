use super::{
    retries::{FixedRetryPolicy, RetryLogic},
    Batch, BatchServiceSink,
};
use crate::buffers::Acker;
use futures::Poll;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tower::{
    layer::{util::Stack, Layer},
    limit::{concurrency::ConcurrencyLimit, rate::RateLimit},
    retry::Retry,
    timeout::Timeout,
    util::BoxService,
    Service, ServiceBuilder,
};

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

    pub fn batch_sink<B, L, S, T>(
        &self,
        retry_logic: L,
        service: S,
        acker: Acker,
    ) -> BatchServiceSink<T, ConcurrencyLimit<RateLimit<Retry<FixedRetryPolicy<L>, Timeout<S>>>>, B>
    // Would like to return `impl Sink + SinkExt<T>` here, but that
    // doesn't work with later calls to `batched_with_min` etc (via
    // `trait SinkExt` above), as it is missing a bound on the
    // associated types that cannot be expressed in stable Rust.
    where
        L: RetryLogic<Error = S::Error, Response = S::Response>,
        S: Clone + Service<T>,
        S::Error: 'static + std::error::Error + Send + Sync,
        S::Response: std::fmt::Debug,
        T: Clone,
        B: Batch<Output = T>,
    {
        let policy = self.retry_policy(retry_logic);
        let service = ServiceBuilder::new()
            .concurrency_limit(self.in_flight_limit)
            .rate_limit(self.rate_limit_num, self.rate_limit_duration)
            .retry(policy)
            .timeout(self.timeout)
            .service(service);

        BatchServiceSink::new(service, acker)
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
    S::Response: Send + 'static,
    S::Error: std::error::Error + Send + Sync + 'static,
    S::Future: Send + 'static,
    L: RetryLogic<Response = S::Response, Error = S::Error> + Send + 'static,
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
            .timeout(self.settings.timeout)
            .service(inner);

        BoxService::new(l)
    }
}

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
    type Future = futures::future::MapErr<S::Future, fn(S::Error) -> crate::Error>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.inner.poll_ready().map_err(Into::into)
    }

    fn call(&mut self, req: R1) -> Self::Future {
        let req = (self.f)(req);
        use futures::Future;
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

#[cfg(test)]
mod tests {
    use super::*;
    use futures::Future;
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
