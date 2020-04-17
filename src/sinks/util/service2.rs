use super::retries2::{FixedRetryPolicy, RetryLogic};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tower03::{
    layer::{util::Stack, Layer},
    util::BoxService,
    Service, ServiceBuilder,
};

// pub type TowerBatchedSink<S, B, L, Request> =
// BatchSink<ConcurrencyLimit<RateLimit<Retry<FixedRetryPolicy<L>, Timeout<S>>>>, B, Request>;

pub trait ServiceBuilderExt<L> {
    fn settings<RL, Request>(
        self,
        settings: TowerRequestSettings,
        retry_logic: RL,
    ) -> ServiceBuilder<Stack<TowerRequestLayer<RL, Request>, L>>;
}

impl<L> ServiceBuilderExt<L> for ServiceBuilder<L> {
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

    // pub fn batch_sink<B, L, S, Request>(
    //     &self,
    //     retry_logic: L,
    //     service: S,
    //     batch: B,
    //     batch_settings: BatchSettings,
    //     acker: Acker,
    // ) -> TowerBatchedSink<S, B, L, Request>
    // // Would like to return `impl Sink + SinkExt<T>` here, but that
    // // doesn't work with later calls to `batched_with_min` etc (via
    // // `trait SinkExt` above), as it is missing a bound on the
    // // associated types that cannot be expressed in stable Rust.
    // where
    //     L: RetryLogic<Response = S::Response> + Send + 'static,
    //     S: Service<Request> + Clone + Send + 'static,
    //     S::Error: Into<crate::Error> + Send + Sync + 'static,
    //     S::Response: Send + std::fmt::Debug,
    //     S::Future: Send + 'static,
    //     B: Batch<Output = Request>,
    //     Request: Send + Clone + 'static,
    // {
    //     let policy = self.retry_policy(retry_logic);
    //     let service = ServiceBuilder::new()
    //         .concurrency_limit(self.in_flight_limit)
    //         .rate_limit(self.rate_limit_num, self.rate_limit_duration)
    //         .retry(policy)
    //         .layer(TimeoutLayer {
    //             timeout: self.timeout,
    //         })
    //         .service(service);

    //     BatchSink::new(service, batch, batch_settings, acker)
    // }
}

#[derive(Debug, Clone)]
pub struct TowerRequestLayer<L, Request> {
    settings: TowerRequestSettings,
    retry_logic: L,
    _pd: std::marker::PhantomData<Request>,
}

impl<S, L, Request> Layer<S> for TowerRequestLayer<L, Request>
where
    S: Service<Request> + Send + Clone + 'static,
    S::Response: Send + 'static,
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
            .timeout(self.settings.timeout)
            .service(inner);

        BoxService::new(l)
    }
}
