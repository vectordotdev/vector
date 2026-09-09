use std::{hash::Hash, marker::PhantomData, num::NonZeroU64, pin::Pin, sync::Arc, time::Duration};

use futures_util::stream::{self, BoxStream};
use serde_with::serde_as;
use tower::{
    Service, ServiceBuilder,
    balance::p2c::Balance,
    buffer::{Buffer, BufferLayer},
    discover::Change,
    layer::{Layer, util::Stack},
    limit::{ConcurrencyLimit, RateLimit},
    retry::Retry,
    timeout::Timeout,
};
use vector_lib::configurable::configurable_component;

pub use crate::sinks::util::service::{
    concurrency::Concurrency,
    health::{HealthConfig, HealthLogic, HealthService},
    map::Map,
};
use crate::{
    internal_events::OpenGauge,
    sinks::util::{
        Batch, BatchSink, Partition, PartitionBatchSink,
        adaptive_concurrency::{
            AdaptiveConcurrencyLimit, AdaptiveConcurrencyLimitLayer, AdaptiveConcurrencySettings,
        },
        retries::{FibonacciRetryPolicy, JitterMode, RetryLogic},
        service::map::MapLayer,
        sink::Response,
    },
};

mod concurrency;
mod health;
mod map;
pub mod net;

pub type Svc<S, L> =
    RateLimit<AdaptiveConcurrencyLimit<Retry<FibonacciRetryPolicy<L>, Timeout<S>>, L>>;
pub type TowerBatchedSink<S, B, RL> = BatchSink<Svc<S, RL>, B>;
pub type TowerPartitionSink<S, B, RL, K> = PartitionBatchSink<Svc<S, RL>, B, K>;

// Distributed service types
pub type DistributedService<S, RL, HL, K, Req> = RateLimit<
    ConcurrencyLimit<
        Retry<
            FibonacciRetryPolicy<RL>,
            Buffer<Req, <Balance<DiscoveryService<S, RL, HL, K>, Req> as Service<Req>>::Future>,
        >,
    >,
>;
pub type DiscoveryService<S, RL, HL, K> =
    BoxStream<'static, Result<Change<K, SingleDistributedService<S, RL, HL>>, crate::Error>>;
pub type SingleDistributedService<S, RL, HL> =
    AdaptiveConcurrencyLimit<HealthService<Timeout<S>, HL>, RL>;

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
        self.layer(MapLayer::new(Arc::new(f)))
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

pub trait TowerRequestConfigDefaults {
    const CONCURRENCY: Concurrency = Concurrency::Adaptive;
    const TIMEOUT_SECS: u64 = 60;
    const RATE_LIMIT_DURATION_SECS: u64 = 1;
    const RATE_LIMIT_NUM: u64 = i64::MAX as u64; // i64 avoids TOML deserialize issue
    const RETRY_ATTEMPTS: usize = isize::MAX as usize; // isize avoids TOML deserialize issue
    const RETRY_MAX_DURATION_SECS: NonZeroU64 = NonZeroU64::new(30).unwrap();
    const RETRY_INITIAL_BACKOFF_SECS: NonZeroU64 = NonZeroU64::new(1).unwrap();
}

#[derive(Clone, Copy, Debug)]
pub struct GlobalTowerRequestConfigDefaults;

impl TowerRequestConfigDefaults for GlobalTowerRequestConfigDefaults {}

/// Middleware settings for outbound requests.
///
/// Various settings can be configured, such as concurrency and rate limits, timeouts, and retry behavior.
///
/// Note that the retry backoff policy follows the Fibonacci sequence.
#[serde_as]
#[configurable_component]
#[derive(Clone, Copy, Debug)]
pub struct TowerRequestConfig<D: TowerRequestConfigDefaults = GlobalTowerRequestConfigDefaults> {
    #[serde(default = "default_concurrency::<D>")]
    #[serde(skip_serializing_if = "concurrency_is_default::<D>")]
    pub concurrency: Concurrency,

    /// The time a request can take before being aborted.
    ///
    /// Datadog highly recommends that you do not lower this value below the service's internal timeout, as this could
    /// create orphaned requests, pile on retries, and result in duplicate data downstream.
    #[configurable(metadata(docs::type_unit = "seconds"))]
    #[configurable(metadata(docs::human_name = "Timeout"))]
    #[serde(default = "default_timeout_secs::<D>")]
    pub timeout_secs: u64,

    /// The time window used for the `rate_limit_num` option.
    #[configurable(metadata(docs::type_unit = "seconds"))]
    #[configurable(metadata(docs::human_name = "Rate Limit Duration"))]
    #[serde(default = "default_rate_limit_duration_secs::<D>")]
    pub rate_limit_duration_secs: u64,

    /// The maximum number of requests allowed within the `rate_limit_duration_secs` time window.
    #[configurable(metadata(docs::type_unit = "requests"))]
    #[configurable(metadata(docs::human_name = "Rate Limit Number"))]
    #[serde(default = "default_rate_limit_num::<D>")]
    pub rate_limit_num: u64,

    /// The maximum number of retries to make for failed requests.
    #[configurable(metadata(docs::type_unit = "retries"))]
    #[serde(default = "default_retry_attempts::<D>")]
    pub retry_attempts: usize,

    /// The maximum amount of time to wait between retries.
    #[configurable(metadata(docs::type_unit = "seconds"))]
    #[configurable(metadata(docs::human_name = "Max Retry Duration"))]
    #[serde(default = "default_retry_max_duration_secs::<D>")]
    pub retry_max_duration_secs: NonZeroU64,

    /// The amount of time to wait before attempting the first retry for a failed request.
    ///
    /// After the first retry has failed, the Fibonacci sequence is used to select future backoffs.
    #[configurable(metadata(docs::type_unit = "seconds"))]
    #[configurable(metadata(docs::human_name = "Retry Initial Backoff"))]
    #[serde(default = "default_retry_initial_backoff_secs::<D>")]
    pub retry_initial_backoff_secs: NonZeroU64,

    #[serde(default)]
    pub retry_jitter_mode: JitterMode,

    #[serde(default)]
    pub adaptive_concurrency: AdaptiveConcurrencySettings,

    #[serde(skip)]
    pub _d: PhantomData<D>,
}

const fn default_concurrency<D: TowerRequestConfigDefaults>() -> Concurrency {
    D::CONCURRENCY
}

fn concurrency_is_default<D: TowerRequestConfigDefaults>(concurrency: &Concurrency) -> bool {
    *concurrency == D::CONCURRENCY
}

const fn default_timeout_secs<D: TowerRequestConfigDefaults>() -> u64 {
    D::TIMEOUT_SECS
}

const fn default_rate_limit_duration_secs<D: TowerRequestConfigDefaults>() -> u64 {
    D::RATE_LIMIT_DURATION_SECS
}

const fn default_rate_limit_num<D: TowerRequestConfigDefaults>() -> u64 {
    D::RATE_LIMIT_NUM
}

const fn default_retry_attempts<D: TowerRequestConfigDefaults>() -> usize {
    D::RETRY_ATTEMPTS
}

const fn default_retry_max_duration_secs<D: TowerRequestConfigDefaults>() -> NonZeroU64 {
    D::RETRY_MAX_DURATION_SECS
}

const fn default_retry_initial_backoff_secs<D: TowerRequestConfigDefaults>() -> NonZeroU64 {
    D::RETRY_INITIAL_BACKOFF_SECS
}

impl<D: TowerRequestConfigDefaults> Default for TowerRequestConfig<D> {
    fn default() -> Self {
        Self {
            concurrency: default_concurrency::<D>(),
            timeout_secs: default_timeout_secs::<D>(),
            rate_limit_duration_secs: default_rate_limit_duration_secs::<D>(),
            rate_limit_num: default_rate_limit_num::<D>(),
            retry_attempts: default_retry_attempts::<D>(),
            retry_max_duration_secs: default_retry_max_duration_secs::<D>(),
            retry_initial_backoff_secs: default_retry_initial_backoff_secs::<D>(),
            adaptive_concurrency: AdaptiveConcurrencySettings::default(),
            retry_jitter_mode: JitterMode::default(),

            _d: PhantomData,
        }
    }
}

impl<D: TowerRequestConfigDefaults> TowerRequestConfig<D> {
    pub const fn into_settings(&self) -> TowerRequestSettings {
        // the unwrap() calls below are safe because the final defaults are always Some<>
        TowerRequestSettings {
            concurrency: self.concurrency.parse_concurrency(),
            timeout: Duration::from_secs(self.timeout_secs),
            rate_limit_duration: Duration::from_secs(self.rate_limit_duration_secs),
            rate_limit_num: self.rate_limit_num,
            retry_attempts: self.retry_attempts,
            retry_max_duration: Duration::from_secs(self.retry_max_duration_secs.get()),
            retry_initial_backoff: Duration::from_secs(self.retry_initial_backoff_secs.get()),
            adaptive_concurrency: self.adaptive_concurrency,
            retry_jitter_mode: self.retry_jitter_mode,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TowerRequestSettings {
    pub concurrency: Option<usize>,
    pub timeout: Duration,
    pub rate_limit_duration: Duration,
    pub rate_limit_num: u64,
    pub retry_attempts: usize,
    pub retry_max_duration: Duration,
    pub retry_initial_backoff: Duration,
    pub adaptive_concurrency: AdaptiveConcurrencySettings,
    pub retry_jitter_mode: JitterMode,
}

impl TowerRequestSettings {
    pub fn retry_policy<L: RetryLogic>(&self, logic: L) -> FibonacciRetryPolicy<L> {
        FibonacciRetryPolicy::new(
            self.retry_attempts,
            self.retry_initial_backoff,
            self.retry_max_duration,
            logic,
            self.retry_jitter_mode,
        )
    }

    /// Note: This has been deprecated, please do not use when creating new Sinks.
    pub fn partition_sink<B, RL, S, K>(
        &self,
        retry_logic: RL,
        service: S,
        batch: B,
        batch_timeout: Duration,
    ) -> TowerPartitionSink<S, B, RL, K>
    where
        RL: RetryLogic<Request = <B as Batch>::Output, Response = S::Response>,
        S: Service<B::Output> + Clone + Send + 'static,
        S::Error: Into<crate::Error> + Send + Sync + 'static,
        S::Response: Send + Response,
        S::Future: Send + 'static,
        B: Batch,
        B::Input: Partition<K>,
        B::Output: Send + Clone + 'static,
        K: Hash + Eq + Clone + Send + 'static,
    {
        let service = ServiceBuilder::new()
            .settings(self.clone(), retry_logic)
            .service(service);
        PartitionBatchSink::new(service, batch, batch_timeout)
    }

    /// Note: This has been deprecated, please do not use when creating new Sinks.
    pub fn batch_sink<B, RL, S>(
        &self,
        retry_logic: RL,
        service: S,
        batch: B,
        batch_timeout: Duration,
    ) -> TowerBatchedSink<S, B, RL>
    where
        RL: RetryLogic<Request = <B as Batch>::Output, Response = S::Response>,
        S: Service<B::Output> + Clone + Send + 'static,
        S::Error: Into<crate::Error> + Send + Sync + 'static,
        S::Response: Send + Response,
        S::Future: Send + 'static,
        B: Batch,
        B::Output: Send + Clone + 'static,
    {
        let service = ServiceBuilder::new()
            .settings(self.clone(), retry_logic)
            .service(service);
        BatchSink::new(service, batch, batch_timeout)
    }

    /// Distributes requests to services [(Endpoint, service, healthcheck)]
    ///
    /// [BufferLayer] suggests that the `buffer_bound` should be at least equal to
    /// the number of the callers of the service. For sinks, this should typically be 1.
    ///
    /// A [ConcurrencyLimit] is placed outside the retry layer so that a request holds a
    /// permit for its whole retry sequence, including the backoff sleeps between attempts.
    /// The per-endpoint [AdaptiveConcurrencyLimit] sits inside the retry layer and releases
    /// its permit as soon as one attempt resolves, so on its own it does not bound the number
    /// of requests that are simultaneously retrying. Without the outer limit the caller sees
    /// a service that is always ready during a sustained downstream failure, and in-flight
    /// retrying requests accumulate without bound.
    ///
    /// The limit is sized at the per-endpoint concurrency ceiling times the number of
    /// endpoints, which is the most the inner limiters can ever admit at once, so it cannot
    /// become the binding constraint on a healthy pipeline.
    pub fn distributed_service<Req, RL, HL, S>(
        self,
        retry_logic: RL,
        services: Vec<(String, S)>,
        health_config: HealthConfig,
        health_logic: HL,
        buffer_bound: usize,
    ) -> DistributedService<S, RL, HL, usize, Req>
    where
        Req: Clone + Send + 'static,
        RL: RetryLogic<Response = S::Response>,
        HL: HealthLogic<Response = S::Response, Error = crate::Error>,
        S: Service<Req> + Clone + Send + 'static,
        S::Error: Into<crate::Error> + Send + Sync + 'static,
        S::Response: Send,
        S::Future: Send + 'static,
    {
        let policy = self.retry_policy(retry_logic.clone());

        // The most requests the per-endpoint limiters can admit at the same time. Adaptive
        // concurrency has no fixed limit, so its configured ceiling is used instead.
        let per_endpoint_limit = self
            .concurrency
            .unwrap_or(self.adaptive_concurrency.max_concurrency_limit);
        let max_in_flight = per_endpoint_limit.saturating_mul(services.len()).max(1);

        // Build services
        let open = OpenGauge::new();
        let services = services
            .into_iter()
            .map(|(endpoint, inner)| {
                // Build individual service
                ServiceBuilder::new()
                    .layer(AdaptiveConcurrencyLimitLayer::new(
                        self.concurrency,
                        self.adaptive_concurrency,
                        retry_logic.clone(),
                    ))
                    .service(
                        health_config.build(
                            health_logic.clone(),
                            ServiceBuilder::new().timeout(self.timeout).service(inner),
                            open.clone(),
                            endpoint,
                        ), // NOTE: there is a version conflict for crate `tracing` between `tracing_tower` crate
                           // and Vector. Once that is resolved, this can be used instead of passing endpoint everywhere.
                           // .trace_service(|_| info_span!("endpoint", %endpoint)),
                    )
            })
            .enumerate()
            .map(|(i, service)| Ok::<_, S::Error>(Change::Insert(i, service)))
            .collect::<Vec<_>>();

        // Build sink service
        ServiceBuilder::new()
            .rate_limit(self.rate_limit_num, self.rate_limit_duration)
            // Outside the retry layer, so the permit spans every attempt and every backoff.
            .concurrency_limit(max_in_flight)
            .retry(policy)
            // [Balance] must be wrapped with a [BufferLayer] so that the overall service implements Clone.
            .layer(BufferLayer::new(buffer_bound))
            .service(Balance::new(Box::pin(stream::iter(services)) as Pin<Box<_>>))
    }
}

#[derive(Debug, Clone)]
pub struct TowerRequestLayer<L, Request> {
    settings: TowerRequestSettings,
    retry_logic: L,
    _pd: PhantomData<Request>,
}

impl<S, RL, Request> Layer<S> for TowerRequestLayer<RL, Request>
where
    S: Service<Request> + Send + 'static,
    S::Response: Send + 'static,
    S::Error: Into<crate::Error> + Send + Sync + 'static,
    S::Future: Send + 'static,
    RL: RetryLogic<Response = S::Response> + Send + 'static,
    Request: Clone + Send + 'static,
{
    type Service = Svc<S, RL>;

    fn layer(&self, inner: S) -> Self::Service {
        let policy = self.settings.retry_policy(self.retry_logic.clone());
        ServiceBuilder::new()
            .rate_limit(
                self.settings.rate_limit_num,
                self.settings.rate_limit_duration,
            )
            .layer(AdaptiveConcurrencyLimitLayer::new(
                self.settings.concurrency,
                self.settings.adaptive_concurrency,
                self.retry_logic.clone(),
            ))
            .retry(policy)
            .timeout(self.settings.timeout)
            .service(inner)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering::AcqRel},
    };

    use futures::{FutureExt, SinkExt, StreamExt, future, stream};
    use tokio::time::Duration;
    use vector_lib::json_size::JsonSize;

    use super::*;
    use crate::sinks::util::{
        BatchSettings, EncodedEvent, PartitionBuffer, PartitionInnerBuffer, VecBuffer,
        retries::{RetryAction, RetryLogic},
    };

    const TIMEOUT: Duration = Duration::from_secs(10);

    #[test]
    fn concurrency_param_works() {
        let cfg = TowerRequestConfig::<GlobalTowerRequestConfigDefaults>::default();
        let toml = toml::to_string(&cfg).unwrap();
        toml::from_str::<TowerRequestConfig>(&toml).expect("Default config failed");

        let cfg = serde_yaml::from_str::<TowerRequestConfig>("").expect("Empty config failed");
        assert_eq!(cfg.concurrency, Concurrency::Adaptive);

        let cfg = serde_yaml::from_str::<TowerRequestConfig>("concurrency: 10")
            .expect("Fixed concurrency failed");
        assert_eq!(cfg.concurrency, Concurrency::Fixed(10));

        let cfg = serde_yaml::from_str::<TowerRequestConfig>(r#"concurrency: "adaptive""#)
            .expect("Adaptive concurrency setting failed");
        assert_eq!(cfg.concurrency, Concurrency::Adaptive);

        let cfg = serde_yaml::from_str::<TowerRequestConfig>(r#"concurrency: "none""#)
            .expect("None concurrency setting failed");
        assert_eq!(cfg.concurrency, Concurrency::None);

        serde_yaml::from_str::<TowerRequestConfig>(r#"concurrency: "broken""#)
            .expect_err("Invalid concurrency setting didn't fail");

        serde_yaml::from_str::<TowerRequestConfig>("concurrency: 0")
            .expect_err("Invalid concurrency setting didn't fail on zero");

        serde_yaml::from_str::<TowerRequestConfig>("concurrency: -9")
            .expect_err("Invalid concurrency setting didn't fail on negative number");
    }

    #[test]
    fn into_settings_with_global_defaults() {
        let cfg = TowerRequestConfig::<GlobalTowerRequestConfigDefaults>::default();
        let settings = cfg.into_settings();

        assert_eq!(settings.concurrency, None);
        assert_eq!(settings.timeout, Duration::from_secs(60));
        assert_eq!(settings.rate_limit_duration, Duration::from_secs(1));
        assert_eq!(settings.rate_limit_num, i64::MAX as u64);
        assert_eq!(settings.retry_attempts, isize::MAX as usize);
        assert_eq!(settings.retry_max_duration, Duration::from_secs(30));
        assert_eq!(settings.retry_initial_backoff, Duration::from_secs(1));
    }

    #[derive(Clone, Copy, Debug)]
    pub struct TestTowerRequestConfigDefaults;

    impl TowerRequestConfigDefaults for TestTowerRequestConfigDefaults {
        const CONCURRENCY: Concurrency = Concurrency::None;
        const TIMEOUT_SECS: u64 = 1;
        const RATE_LIMIT_DURATION_SECS: u64 = 2;
        const RATE_LIMIT_NUM: u64 = 3;
        const RETRY_ATTEMPTS: usize = 4;
        const RETRY_MAX_DURATION_SECS: NonZeroU64 = NonZeroU64::new(5).unwrap();
        const RETRY_INITIAL_BACKOFF_SECS: NonZeroU64 = NonZeroU64::new(6).unwrap();
    }

    #[test]
    fn into_settings_with_overridden_defaults() {
        let cfg = TowerRequestConfig::<TestTowerRequestConfigDefaults>::default();
        let settings = cfg.into_settings();

        assert_eq!(settings.concurrency, Some(1));
        assert_eq!(settings.timeout, Duration::from_secs(1));
        assert_eq!(settings.rate_limit_duration, Duration::from_secs(2));
        assert_eq!(settings.rate_limit_num, 3);
        assert_eq!(settings.retry_attempts, 4);
        assert_eq!(settings.retry_max_duration, Duration::from_secs(5));
        assert_eq!(settings.retry_initial_backoff, Duration::from_secs(6));
    }

    #[test]
    fn into_settings_with_populated_config() {
        // Populate with values not equal to the global defaults.
        let cfg = serde_yaml::from_str::<TowerRequestConfig>(indoc::indoc! {"
            concurrency: 16
            timeout_secs: 1
            rate_limit_duration_secs: 2
            rate_limit_num: 3
            retry_attempts: 4
            retry_max_duration_secs: 5
            retry_initial_backoff_secs: 6
        "})
        .expect("Config failed to parse");

        // Merge with defaults
        let settings = cfg.into_settings();
        assert_eq!(
            settings.concurrency,
            Concurrency::Fixed(16).parse_concurrency()
        );
        assert_eq!(settings.timeout, Duration::from_secs(1));
        assert_eq!(settings.rate_limit_duration, Duration::from_secs(2));
        assert_eq!(settings.rate_limit_num, 3);
        assert_eq!(settings.retry_attempts, 4);
        assert_eq!(settings.retry_max_duration, Duration::from_secs(5));
        assert_eq!(settings.retry_initial_backoff, Duration::from_secs(6));
    }

    #[tokio::test]
    async fn partition_sink_retry_concurrency() {
        let cfg: TowerRequestConfig<GlobalTowerRequestConfigDefaults> = TowerRequestConfig {
            concurrency: Concurrency::Fixed(1),
            ..TowerRequestConfig::default()
        };
        let settings = cfg.into_settings();

        let sent_requests = Arc::new(Mutex::new(Vec::new()));

        let svc = {
            let sent_requests = Arc::clone(&sent_requests);
            let delay = Arc::new(AtomicBool::new(true));
            tower::service_fn(move |req: PartitionInnerBuffer<Vec<usize>, Vec<usize>>| {
                let (req, _) = req.into_parts();
                if delay.swap(false, AcqRel) {
                    // Error on first request
                    future::err::<(), _>(std::io::Error::other("")).boxed()
                } else {
                    sent_requests.lock().unwrap().push(req);
                    future::ok::<_, std::io::Error>(()).boxed()
                }
            })
        };

        let mut batch_settings = BatchSettings::default();
        batch_settings.size.bytes = 9999;
        batch_settings.size.events = 10;

        let mut sink = settings.partition_sink(
            RetryAlways,
            svc,
            PartitionBuffer::new(VecBuffer::new(batch_settings.size)),
            TIMEOUT,
        );
        sink.ordered();

        let input = (0..20).map(|i| PartitionInnerBuffer::new(i, vec![0]));
        sink.sink_map_err(drop)
            .send_all(
                &mut stream::iter(input)
                    .map(|item| Ok(EncodedEvent::new(item, 0, JsonSize::zero()))),
            )
            .await
            .unwrap();

        let output = sent_requests.lock().unwrap();
        assert_eq!(
            &*output,
            &vec![(0..10).collect::<Vec<_>>(), (10..20).collect::<Vec<_>>(),]
        );
    }

    #[derive(Clone, Debug, Copy)]
    struct RetryAlways;

    impl RetryLogic for RetryAlways {
        type Error = std::io::Error;
        type Request = PartitionInnerBuffer<Vec<usize>, Vec<usize>>;
        type Response = ();

        fn is_retriable_error(&self, _: &Self::Error) -> bool {
            true
        }

        fn should_retry_response(&self, _response: &Self::Response) -> RetryAction<Self::Request> {
            // Treat the default as the request is successful
            RetryAction::Successful
        }
    }
}

/// Backpressure coverage for [`TowerRequestSettings::distributed_service`].
///
/// The endpoints answer instantly with a retriable status and nothing ever succeeds, so every
/// request the service accepts stays resident in the caller's in-flight set for the duration of
/// the outage. The question these tests ask is the one the sink driver asks on every iteration:
/// how many requests will the service accept before `poll_ready` stops returning `Ready`?
#[cfg(test)]
mod distributed_backpressure_tests {
    use std::task::{Context, Poll};

    use futures::{StreamExt, future, stream::FuturesUnordered};
    use http::StatusCode;
    use tower::Service;

    use super::{
        Concurrency, GlobalTowerRequestConfigDefaults, HealthConfig, HealthLogic,
        TowerRequestConfig, TowerRequestSettings,
    };
    use crate::sinks::util::retries::{JitterMode, RetryAction, RetryLogic};

    const ENDPOINTS: usize = 2;
    const PER_ENDPOINT: usize = 3;

    /// What the outer concurrency limit is sized at: the most the per-endpoint limiters can
    /// ever admit at the same time.
    const EXPECTED_BOUND: usize = ENDPOINTS * PER_ENDPOINT;

    /// Far above the bound, so a service that never applies backpressure runs to the cap
    /// instead of settling.
    const ADMISSION_CAP: usize = 500;

    /// Rounds of no progress before the harness accepts that backpressure has engaged.
    const IDLE_ROUNDS: usize = 64;

    #[derive(Clone, Debug)]
    struct FaultRequest;

    #[derive(Clone, Debug)]
    struct FaultResponse(StatusCode);

    #[derive(Debug)]
    struct FaultError;

    impl std::fmt::Display for FaultError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "fault")
        }
    }

    impl std::error::Error for FaultError {}

    /// An endpoint that answers every request immediately with a fixed status.
    #[derive(Clone)]
    struct FaultService(StatusCode);

    impl Service<FaultRequest> for FaultService {
        type Response = FaultResponse;
        type Error = crate::Error;
        type Future = future::Ready<Result<FaultResponse, crate::Error>>;

        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, _req: FaultRequest) -> Self::Future {
            future::ready(Ok(FaultResponse(self.0)))
        }
    }

    /// Mirrors `ElasticsearchRetryLogic`: 429 and 5xx are retried, and with the default
    /// `retry_attempts` they are retried forever.
    #[derive(Clone)]
    struct FaultRetryLogic;

    impl RetryLogic for FaultRetryLogic {
        type Error = FaultError;
        type Request = FaultRequest;
        type Response = FaultResponse;

        fn is_retriable_error(&self, _error: &Self::Error) -> bool {
            true
        }

        fn should_retry_response(&self, response: &FaultResponse) -> RetryAction<FaultRequest> {
            if response.0 == StatusCode::TOO_MANY_REQUESTS || response.0.is_server_error() {
                RetryAction::Retry("fault injected".into())
            } else {
                RetryAction::Successful
            }
        }
    }

    /// Mirrors `ElasticsearchHealthLogic`: 5xx counts against the endpoint, 4xx is not
    /// classified at all, so the per-endpoint circuit breaker never sees it.
    #[derive(Clone)]
    struct FaultHealthLogic;

    impl HealthLogic for FaultHealthLogic {
        type Error = crate::Error;
        type Response = FaultResponse;

        fn is_healthy(&self, response: &Result<FaultResponse, crate::Error>) -> Option<bool> {
            match response {
                Ok(response) if response.0.is_success() => Some(true),
                Ok(response) if response.0.is_server_error() => Some(false),
                Ok(_) => None,
                Err(_) => None,
            }
        }
    }

    fn settings() -> TowerRequestSettings {
        TowerRequestConfig::<GlobalTowerRequestConfigDefaults> {
            concurrency: Concurrency::Fixed(PER_ENDPOINT),
            // Deterministic backoff, so the measurement does not depend on jitter.
            retry_jitter_mode: JitterMode::None,
            ..TowerRequestConfig::default()
        }
        .into_settings()
    }

    /// Drives the service the way `Driver::run` does and returns how many requests it accepted.
    ///
    /// Time is paused and the harness task is always runnable, so no retry backoff and no
    /// circuit breaker backoff ever elapses. Every accepted request is therefore still parked
    /// mid retry when the next one is offered.
    async fn admitted_before_backpressure(status: StatusCode) -> usize {
        let services = (0..ENDPOINTS)
            .map(|i| (format!("endpoint-{i}"), FaultService(status)))
            .collect::<Vec<_>>();

        let mut service = settings().distributed_service(
            FaultRetryLogic,
            services,
            HealthConfig::default(),
            FaultHealthLogic,
            // Same buffer bound the Elasticsearch sink uses.
            1,
        );

        let mut in_flight = FuturesUnordered::new();
        let mut admitted = 0usize;
        let mut idle_rounds = 0usize;

        while admitted < ADMISSION_CAP && idle_rounds < IDLE_ROUNDS {
            // Let the buffer worker and the endpoint services run.
            for _ in 0..4 {
                tokio::task::yield_now().await;
            }

            // Drain anything that resolved, which is what lets the driver submit more.
            while let Poll::Ready(Some(_)) = futures::poll!(in_flight.next()) {}

            match futures::poll!(future::poll_fn(|cx| service.poll_ready(cx))) {
                Poll::Ready(Ok(())) => {
                    in_flight.push(service.call(FaultRequest));
                    admitted += 1;
                    idle_rounds = 0;
                }
                Poll::Ready(Err(error)) => panic!("service returned an error: {error}"),
                Poll::Pending => idle_rounds += 1,
            }
        }

        admitted
    }

    /// A sustained 429 is retried forever by the retry logic and is never classified by the
    /// health logic, so the per-endpoint circuit breaker cannot trip on it. Before the outer
    /// concurrency limit existed this ran to `ADMISSION_CAP` with every request still in
    /// flight, which is the unbounded growth reported in #25212.
    #[tokio::test(start_paused = true)]
    async fn too_many_requests_applies_backpressure() {
        let admitted = admitted_before_backpressure(StatusCode::TOO_MANY_REQUESTS).await;

        assert!(
            admitted <= EXPECTED_BOUND,
            "sustained 429 admitted {admitted} concurrent retrying requests, expected at most \
             {EXPECTED_BOUND}"
        );
    }

    /// A sustained 5xx is also retried forever. The circuit breaker does classify it, so this
    /// case has always settled, but it must settle at or below the same bound.
    #[tokio::test(start_paused = true)]
    async fn server_error_applies_backpressure() {
        let admitted = admitted_before_backpressure(StatusCode::SERVICE_UNAVAILABLE).await;

        assert!(
            admitted <= EXPECTED_BOUND,
            "sustained 503 admitted {admitted} concurrent retrying requests, expected at most \
             {EXPECTED_BOUND}"
        );
    }
}
