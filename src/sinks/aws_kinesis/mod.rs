pub mod data_firehose;
pub mod data_streams;

use crate::{
    buffers::Acker,
    event::{self, Event},
    sinks::util::{
        retries::{FixedRetryPolicy, RetryLogic},
        BatchServiceSink, SinkExt,
    },
};
use futures::{stream::iter_ok, Sink};
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use std::error::Error;
use std::time::Duration;
use tower::{Service, ServiceBuilder};

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone, Derivative)]
#[serde(rename_all = "snake_case")]
#[derivative(Default)]
pub enum Encoding {
    #[derivative(Default)]
    Text,
    Json,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct CoreSinkConfig {
    pub encoding: Encoding,
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
    #[serde(default = "default_batch_timeout")]
    pub batch_timeout: u64,
}

impl Default for CoreSinkConfig {
    fn default() -> Self {
        CoreSinkConfig {
            encoding: Encoding::Text,
            batch_size: default_batch_size(),
            batch_timeout: default_batch_timeout(),
        }
    }
}

pub fn default_batch_size() -> usize {
    bytesize::mib(1u64) as usize
}
pub fn default_batch_timeout() -> u64 {
    1
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct TowerRequestConfig {
    #[serde(default = "default_in_flight_limit")]
    pub in_flight_limit: usize,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
    #[serde(default = "default_rate_limit_duration_secs")]
    pub rate_limit_duration_secs: u64,
    #[serde(default = "default_rate_limit_num")]
    pub rate_limit_num: u64,
    #[serde(default = "default_retry_backoff_secs")]
    pub retry_backoff_secs: u64,
    #[serde(default = "default_retry_attempts")]
    pub retry_attempts: usize,
}

impl Default for TowerRequestConfig {
    fn default() -> Self {
        TowerRequestConfig {
            in_flight_limit: default_in_flight_limit(),
            timeout_secs: default_timeout_secs(),
            rate_limit_duration_secs: default_rate_limit_duration_secs(),
            rate_limit_num: default_rate_limit_num(),
            retry_backoff_secs: default_retry_backoff_secs(),
            retry_attempts: default_retry_attempts(),
        }
    }
}

pub fn default_in_flight_limit() -> usize {
    5
}
pub fn default_timeout_secs() -> u64 {
    30
}
pub fn default_rate_limit_duration_secs() -> u64 {
    1
}
pub fn default_rate_limit_num() -> u64 {
    5
}
pub fn default_retry_backoff_secs() -> u64 {
    5
}
pub fn default_retry_attempts() -> usize {
    usize::max_value()
}

#[derive(Debug, Snafu)]
pub enum HealthcheckError<E: Error>
where
    E: 'static,
{
    #[snafu(display("Retrieval of stream description failed: {}", source))]
    StreamRetrievalFailed { source: E },
    #[snafu(display("The stream {} is not found", stream_name))]
    NoMatchingStreamName { stream_name: String },
    #[snafu(display("The stream {} is not ready to receive input", stream_name))]
    StreamIsNotReady { stream_name: String },
}

fn construct<L, T, S, F>(
    core_config: CoreSinkConfig,
    request_config: TowerRequestConfig,
    service: S,
    acker: Acker,
    policy: FixedRetryPolicy<L>,
    encode_event: F,
) -> crate::Result<impl Sink<SinkItem = Event, SinkError = ()>>
where
    T: Clone,
    L: RetryLogic<Response = S::Response>,
    F: Fn(Event, &Encoding) -> Option<T>,
    S: Service<Vec<T>> + Clone + Sync,
    S::Error: 'static + std::error::Error + Send + Sync,
    S::Response: std::fmt::Debug,
{
    let service = ServiceBuilder::new()
        .concurrency_limit(request_config.in_flight_limit)
        .rate_limit(
            request_config.rate_limit_num,
            Duration::from_secs(request_config.rate_limit_duration_secs),
        )
        .retry(policy)
        .timeout(Duration::from_secs(request_config.timeout_secs))
        .service(service);

    let encoding = core_config.encoding.clone();
    let sink = BatchServiceSink::new(service, acker)
        .batched_with_min(
            Vec::new(),
            core_config.batch_size,
            Duration::from_secs(core_config.batch_timeout),
        )
        .with_flat_map(move |e| iter_ok(encode_event(e, &encoding)));

    Ok(sink)
}

fn encode_event(event: Event, encoding: &Encoding) -> Vec<u8> {
    let log = event.into_log();

    match encoding {
        Encoding::Json => {
            serde_json::to_vec(&log.unflatten()).expect("Error encoding event as json.")
        }

        Encoding::Text => log
            .get(&event::MESSAGE)
            .map(|v| v.as_bytes().to_vec())
            .unwrap_or_default(),
    }
}
