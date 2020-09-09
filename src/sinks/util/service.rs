use super::auto_concurrency::{
    AutoConcurrencyLimit, AutoConcurrencyLimitLayer, AutoConcurrencySettings,
};
use super::retries::{FixedRetryPolicy, RetryLogic};
use super::sink::Response;
use super::{Batch, BatchSink};
use crate::buffers::Acker;
use futures::TryFutureExt;
use serde::{
    de::{self, Unexpected, Visitor},
    Deserialize, Deserializer, Serialize,
};
use std::fmt;
use std::sync::Arc;
use std::task::Poll;
use std::time::Duration;
use tower::{
    layer::{util::Stack, Layer},
    limit::RateLimit,
    retry::Retry,
    timeout::Timeout,
    util::BoxService,
    Service, ServiceBuilder,
};

pub type Svc<S, L> = RateLimit<Retry<FixedRetryPolicy<L>, AutoConcurrencyLimit<Timeout<S>, L>>>;
pub type TowerBatchedSink<S, B, L, Request> = BatchSink<Svc<S, L>, B, Request>;

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
pub enum InFlightLimit {
    #[derivative(Default)]
    None,
    Auto,
    Fixed(usize),
}

impl InFlightLimit {
    pub fn if_none(self, other: Self) -> Self {
        match self {
            Self::None => other,
            _ => self,
        }
    }
}

impl<'de> Deserialize<'de> for InFlightLimit {
    // Deserialize either a positive integer or the string "auto"
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct UsizeOrAuto;

        impl<'de> Visitor<'de> for UsizeOrAuto {
            type Value = InFlightLimit;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str(r#"positive integer or "auto""#)
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<InFlightLimit, E> {
                if value == "auto" {
                    Ok(InFlightLimit::Auto)
                } else {
                    Err(de::Error::unknown_variant(value, &["auto"]))
                }
            }

            fn visit_i64<E: de::Error>(self, value: i64) -> Result<InFlightLimit, E> {
                if value > 0 {
                    Ok(InFlightLimit::Fixed(value as usize))
                } else {
                    Err(de::Error::invalid_value(
                        Unexpected::Signed(value),
                        &"positive integer",
                    ))
                }
            }

            fn visit_u64<E: de::Error>(self, value: u64) -> Result<InFlightLimit, E> {
                if value > 0 {
                    Ok(InFlightLimit::Fixed(value as usize))
                } else {
                    Err(de::Error::invalid_value(
                        Unexpected::Unsigned(value),
                        &"positive integer",
                    ))
                }
            }
        }

        deserializer.deserialize_any(UsizeOrAuto)
    }
}

pub trait InFlightLimitOption {
    fn parse_in_flight_limit(&self, default: &Self) -> Option<usize>;
}

impl InFlightLimitOption for Option<usize> {
    fn parse_in_flight_limit(&self, default: &Self) -> Option<usize> {
        let limit = match self {
            None => *default,
            Some(x) => Some(*x),
        };
        limit.or(Some(5))
    }
}

impl InFlightLimitOption for InFlightLimit {
    fn parse_in_flight_limit(&self, default: &Self) -> Option<usize> {
        match self.if_none(*default) {
            InFlightLimit::None => Some(5),
            InFlightLimit::Auto => None,
            InFlightLimit::Fixed(limit) => Some(limit),
        }
    }
}

/// Tower Request based configuration
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
pub struct TowerRequestConfig<T = InFlightLimit> {
    #[serde(default)]
    pub in_flight_limit: T, // 5
    pub timeout_secs: Option<u64>,             // 60
    pub rate_limit_duration_secs: Option<u64>, // 1
    pub rate_limit_num: Option<u64>,           // 5
    pub retry_attempts: Option<usize>,         // max_value()
    pub retry_max_duration_secs: Option<u64>,
    pub retry_initial_backoff_secs: Option<u64>, // 1
    #[serde(default)]
    pub auto_concurrency: AutoConcurrencySettings,
}

impl<T: InFlightLimitOption> TowerRequestConfig<T> {
    pub fn unwrap_with(&self, defaults: &Self) -> TowerRequestSettings {
        TowerRequestSettings {
            in_flight_limit: self
                .in_flight_limit
                .parse_in_flight_limit(&defaults.in_flight_limit),
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
            auto_concurrency: self.auto_concurrency,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TowerRequestSettings {
    pub in_flight_limit: Option<usize>,
    pub timeout: Duration,
    pub rate_limit_duration: Duration,
    pub rate_limit_num: u64,
    pub retry_attempts: usize,
    pub retry_max_duration_secs: Duration,
    pub retry_initial_backoff_secs: Duration,
    pub auto_concurrency: AutoConcurrencySettings,
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
        batch_timeout: Duration,
        acker: Acker,
    ) -> TowerBatchedSink<S, B, L, Request>
    // Would like to return `impl Sink + SinkExt<T>` here, but that
    // doesn't work with later calls to `batched_with_min` etc (via
    // `trait SinkExt` above), as it is missing a bound on the
    // associated types that cannot be expressed in stable Rust.
    where
        L: RetryLogic<Response = S::Response>,
        S: Service<Request> + Clone + Send + 'static,
        S::Error: Into<crate::Error> + Send + Sync + 'static,
        S::Response: Send + Response,
        S::Future: Send + 'static,
        B: Batch<Output = Request>,
        Request: Send + Clone + 'static,
    {
        let policy = self.retry_policy(retry_logic.clone());
        let service = ServiceBuilder::new()
            .rate_limit(self.rate_limit_num, self.rate_limit_duration)
            .retry(policy)
            .layer(AutoConcurrencyLimitLayer::new(
                self.in_flight_limit,
                self.auto_concurrency,
                retry_logic,
            ))
            .timeout(self.timeout)
            .service(service);

        BatchSink::new(service, batch, batch_timeout, acker)
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
            .concurrency_limit(self.settings.in_flight_limit.unwrap_or(5))
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

    fn poll_ready(&mut self, cx: &mut std::task::Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner
            .poll_ready(cx)
            .map(|result| result.map_err(|e| e.into()))
    }

    fn call(&mut self, req: R1) -> Self::Future {
        let req = (self.f)(req);
        self.inner.call(req).map_err(Into::into)
    }
}

impl<S: Clone, R1, R2> Clone for Map<S, R1, R2> {
    fn clone(&self) -> Self {
        Self {
            f: Arc::clone(&self.f),
            inner: self.inner.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_flight_limit_works() {
        type TowerRequestConfigTest = TowerRequestConfig<InFlightLimit>;

        let cfg = toml::from_str::<TowerRequestConfigTest>("").expect("Empty config failed");
        assert_eq!(cfg.in_flight_limit, InFlightLimit::None);

        let cfg = toml::from_str::<TowerRequestConfigTest>("in_flight_limit = 10")
            .expect("Fixed in_flight_limit failed");
        assert_eq!(cfg.in_flight_limit, InFlightLimit::Fixed(10));

        let cfg = toml::from_str::<TowerRequestConfigTest>(r#"in_flight_limit = "auto""#)
            .expect("Auto in_flight_limit failed");
        assert_eq!(cfg.in_flight_limit, InFlightLimit::Auto);

        toml::from_str::<TowerRequestConfigTest>(r#"in_flight_limit = "broken""#)
            .expect_err("Invalid in_flight_limit didn't fail");

        toml::from_str::<TowerRequestConfigTest>(r#"in_flight_limit = 0"#)
            .expect_err("Invalid in_flight_limit didn't fail on zero");

        toml::from_str::<TowerRequestConfigTest>(r#"in_flight_limit = -9"#)
            .expect_err("Invalid in_flight_limit didn't fail on negative number");
    }
}
