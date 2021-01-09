use super::{
    adaptive_concurrency::{
        AdaptiveConcurrencyLimit, AdaptiveConcurrencyLimitLayer, AdaptiveConcurrencySettings,
    },
    retries::{FixedRetryPolicy, RetryLogic},
    sink::Response,
    Batch, BatchSink, Partition, PartitionBatchSink,
};
use crate::buffers::Acker;
use serde::{
    de::{self, Unexpected, Visitor},
    Deserialize, Deserializer, Serialize,
};
use std::{fmt, hash::Hash, sync::Arc, task::Poll, time::Duration};
use tower::{
    layer::{util::Stack, Layer},
    limit::RateLimit,
    retry::Retry,
    timeout::Timeout,
    util::BoxService,
    Service, ServiceBuilder,
};

pub type Svc<S, L> = RateLimit<Retry<FixedRetryPolicy<L>, AdaptiveConcurrencyLimit<Timeout<S>, L>>>;
pub type TowerBatchedSink<S, B, L, Request> = BatchSink<Svc<S, L>, B, Request>;
pub type TowerPartitionSink<S, B, L, K, Request> = PartitionBatchSink<Svc<S, L>, B, K, Request>;

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

#[derive(Clone, Copy, Debug, Derivative, Eq, PartialEq, Serialize)]
#[derivative(Default)]
pub enum Concurrency {
    #[derivative(Default)]
    None,
    Adaptive,
    Fixed(usize),
}

impl Concurrency {
    pub fn if_none(self, other: Self) -> Self {
        match self {
            Self::None => other,
            _ => self,
        }
    }
}

impl<'de> Deserialize<'de> for Concurrency {
    // Deserialize either a positive integer or the string "adaptive"
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct UsizeOrAdaptive;

        impl<'de> Visitor<'de> for UsizeOrAdaptive {
            type Value = Concurrency;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str(r#"positive integer or "adaptive""#)
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<Concurrency, E> {
                if value == "adaptive" {
                    Ok(Concurrency::Adaptive)
                } else {
                    Err(de::Error::unknown_variant(value, &["adaptive"]))
                }
            }

            fn visit_i64<E: de::Error>(self, value: i64) -> Result<Concurrency, E> {
                if value > 0 {
                    Ok(Concurrency::Fixed(value as usize))
                } else {
                    Err(de::Error::invalid_value(
                        Unexpected::Signed(value),
                        &"positive integer",
                    ))
                }
            }

            fn visit_u64<E: de::Error>(self, value: u64) -> Result<Concurrency, E> {
                if value > 0 {
                    Ok(Concurrency::Fixed(value as usize))
                } else {
                    Err(de::Error::invalid_value(
                        Unexpected::Unsigned(value),
                        &"positive integer",
                    ))
                }
            }
        }

        deserializer.deserialize_any(UsizeOrAdaptive)
    }
}

pub trait ConcurrencyOption {
    fn parse_concurrency(&self, default: &Self) -> Option<usize>;
    fn is_none(&self) -> bool;
    fn is_some(&self) -> bool {
        !self.is_none()
    }
}

impl ConcurrencyOption for Option<usize> {
    fn parse_concurrency(&self, default: &Self) -> Option<usize> {
        let limit = match self {
            None => *default,
            Some(x) => Some(*x),
        };
        limit.or(Some(5))
    }

    fn is_none(&self) -> bool {
        matches!(self, None)
    }
}

impl ConcurrencyOption for Concurrency {
    fn parse_concurrency(&self, default: &Self) -> Option<usize> {
        match self.if_none(*default) {
            Concurrency::None => Some(5),
            Concurrency::Adaptive => None,
            Concurrency::Fixed(limit) => Some(limit),
        }
    }

    fn is_none(&self) -> bool {
        matches!(self, Concurrency::None)
    }
}

/// Tower Request based configuration
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
pub struct TowerRequestConfig<T: ConcurrencyOption = Concurrency> {
    #[serde(default)]
    #[serde(skip_serializing_if = "ConcurrencyOption::is_none")]
    pub concurrency: T, // 5
    /// The same as concurrency but with old deprecated name.
    /// Alias couldn't be used because of https://github.com/serde-rs/serde/issues/1504
    #[serde(default)]
    #[serde(skip_serializing_if = "ConcurrencyOption::is_none")]
    pub in_flight_limit: T, // 5
    pub timeout_secs: Option<u64>,             // 60
    pub rate_limit_duration_secs: Option<u64>, // 1
    pub rate_limit_num: Option<u64>,           // 5
    pub retry_attempts: Option<usize>,         // max_value()
    pub retry_max_duration_secs: Option<u64>,
    pub retry_initial_backoff_secs: Option<u64>, // 1
    #[serde(default)]
    pub adaptive_concurrency: AdaptiveConcurrencySettings,
}

impl<T: ConcurrencyOption> TowerRequestConfig<T> {
    pub fn unwrap_with(&self, defaults: &Self) -> TowerRequestSettings {
        TowerRequestSettings {
            concurrency: self
                .concurrency()
                .parse_concurrency(&defaults.concurrency()),
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
            adaptive_concurrency: self.adaptive_concurrency,
        }
    }

    pub fn concurrency(&self) -> &T {
        match (self.concurrency.is_some(), self.in_flight_limit.is_some()) {
            (_, false) => &self.concurrency,
            (false, true) => &self.in_flight_limit,
            (true, true) => {
                warn!("Option `in_flight_limit` has been renamed to `concurrency`. Ignoring `in_flight_limit` and using `concurrency` option.");
                &self.concurrency
            }
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
    pub retry_max_duration_secs: Duration,
    pub retry_initial_backoff_secs: Duration,
    pub adaptive_concurrency: AdaptiveConcurrencySettings,
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

    pub fn partition_sink<B, L, S, K, Request>(
        &self,
        retry_logic: L,
        service: S,
        batch: B,
        batch_timeout: Duration,
        acker: Acker,
    ) -> TowerPartitionSink<S, B, L, K, Request>
    where
        L: RetryLogic<Response = S::Response>,
        S: Service<Request> + Clone + Send + 'static,
        S::Error: Into<crate::Error> + Send + Sync + 'static,
        S::Response: Send + Response,
        S::Future: Send + 'static,
        B: Batch<Output = Request>,
        B::Input: Partition<K>,
        K: Hash + Eq + Clone + Send + 'static,
        Request: Send + Clone + 'static,
    {
        PartitionBatchSink::new(
            self.service(retry_logic, service),
            batch,
            batch_timeout,
            acker,
        )
    }

    pub fn batch_sink<B, L, S, Request>(
        &self,
        retry_logic: L,
        service: S,
        batch: B,
        batch_timeout: Duration,
        acker: Acker,
    ) -> TowerBatchedSink<S, B, L, Request>
    where
        L: RetryLogic<Response = S::Response>,
        S: Service<Request> + Clone + Send + 'static,
        S::Error: Into<crate::Error> + Send + Sync + 'static,
        S::Response: Send + Response,
        S::Future: Send + 'static,
        B: Batch<Output = Request>,
        Request: Send + Clone + 'static,
    {
        BatchSink::new(
            self.service(retry_logic, service),
            batch,
            batch_timeout,
            acker,
        )
    }

    pub fn service<L, S, Request>(&self, retry_logic: L, service: S) -> Svc<S, L>
    where
        L: RetryLogic<Response = S::Response>,
        S: Service<Request> + Clone + Send + 'static,
        S::Error: Into<crate::Error> + Send + Sync + 'static,
        S::Response: Send + Response,
        S::Future: Send + 'static,
        Request: Send + Clone + 'static,
    {
        let policy = self.retry_policy(retry_logic.clone());
        ServiceBuilder::new()
            .rate_limit(self.rate_limit_num, self.rate_limit_duration)
            .retry(policy)
            .layer(AdaptiveConcurrencyLimitLayer::new(
                self.concurrency,
                self.adaptive_concurrency,
                retry_logic,
            ))
            .timeout(self.timeout)
            .service(service)
    }
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
            .concurrency_limit(self.settings.concurrency.unwrap_or(5))
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
            f: Arc::clone(&self.f),
            inner,
        }
    }
}

pub struct Map<S, R1, R2> {
    f: Arc<dyn Fn(R1) -> R2 + Send + Sync + 'static>,
    pub(crate) inner: S,
}

impl<S, R1, R2> Service<R1> for Map<S, R1, R2>
where
    S: Service<R2>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn poll_ready(&mut self, cx: &mut std::task::Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: R1) -> Self::Future {
        let req = (self.f)(req);
        self.inner.call(req)
    }
}

impl<S, R1, R2> Clone for Map<S, R1, R2>
where
    S: Clone,
{
    fn clone(&self) -> Self {
        Self {
            f: Arc::clone(&self.f),
            inner: self.inner.clone(),
        }
    }
}

impl<S, R1, R2> fmt::Debug for Map<S, R1, R2>
where
    S: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Map").field("inner", &self.inner).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concurrency_param_works() {
        type TowerRequestConfigTest = TowerRequestConfig<Concurrency>;

        let cfg = TowerRequestConfigTest::default();
        let toml = toml::to_string(&cfg).unwrap();
        toml::from_str::<TowerRequestConfigTest>(&toml).expect("Default config failed");

        let cfg = toml::from_str::<TowerRequestConfigTest>("").expect("Empty config failed");
        assert_eq!(cfg.concurrency, Concurrency::None);

        let cfg = toml::from_str::<TowerRequestConfigTest>("concurrency = 10")
            .expect("Fixed concurrency failed");
        assert_eq!(cfg.concurrency, Concurrency::Fixed(10));

        let cfg = toml::from_str::<TowerRequestConfigTest>(r#"concurrency = "adaptive""#)
            .expect("Adaptive concurrency setting failed");
        assert_eq!(cfg.concurrency, Concurrency::Adaptive);

        toml::from_str::<TowerRequestConfigTest>(r#"concurrency = "broken""#)
            .expect_err("Invalid concurrency setting didn't fail");

        toml::from_str::<TowerRequestConfigTest>(r#"concurrency = 0"#)
            .expect_err("Invalid concurrency setting didn't fail on zero");

        toml::from_str::<TowerRequestConfigTest>(r#"concurrency = -9"#)
            .expect_err("Invalid concurrency setting didn't fail on negative number");
    }

    #[test]
    fn backward_compatibility_with_in_flight_limit_param_works() {
        type TowerRequestConfigTest = TowerRequestConfig<Concurrency>;

        let cfg = toml::from_str::<TowerRequestConfigTest>("in_flight_limit = 10")
            .expect("Fixed concurrency failed for in_flight_limit param");
        assert_eq!(cfg.concurrency(), &Concurrency::Fixed(10));
    }
}
