use crate::{
    buffers::Acker,
    event::Event,
    region::RegionOrEndpoint,
    sinks::util::{
        retries::{FixedRetryPolicy, RetryLogic},
        BatchServiceSink, SinkExt,
    },
    topology::config::{DataType, SinkConfig},
};
use futures::{future, Poll, Sink};
use rusoto_cloudwatch::{CloudWatch, CloudWatchClient, PutMetricDataError, PutMetricDataInput};
use rusoto_core::RusotoFuture;
use serde::{Deserialize, Serialize};
use std::{convert::TryInto, fmt, time::Duration};
use tower::{Service, ServiceBuilder};
use tracing::field;
use tracing_futures::{Instrument, Instrumented};

#[derive(Clone)]
pub struct CloudWatchMetricsService {
    client: CloudWatchClient,
    config: CloudWatchMetricsSinkConfig,
}

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct CloudWatchMetricsSinkConfig {
    pub namespace: String,
    #[serde(flatten)]
    pub region: RegionOrEndpoint,
    pub batch_size: Option<usize>,
    pub batch_timeout: Option<u64>,

    // Tower Request based configuration
    pub request_in_flight_limit: Option<usize>,
    pub request_timeout_secs: Option<u64>,
    pub request_rate_limit_duration_secs: Option<u64>,
    pub request_rate_limit_num: Option<u64>,
    pub request_retry_attempts: Option<usize>,
    pub request_retry_backoff_secs: Option<u64>,
}

#[typetag::serde(name = "aws_cloudwatch_metrics")]
impl SinkConfig for CloudWatchMetricsSinkConfig {
    fn build(&self, acker: Acker) -> Result<(super::RouterSink, super::Healthcheck), String> {
        let config = self.clone();
        let sink = CloudWatchMetricsService::new(config, acker)?;
        let healthcheck = CloudWatchMetricsService::healthcheck(self)?;
        Ok((Box::new(sink), healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Metric
    }
}

impl CloudWatchMetricsService {
    pub fn new(
        config: CloudWatchMetricsSinkConfig,
        acker: Acker,
    ) -> Result<impl Sink<SinkItem = Event, SinkError = ()>, String> {
        let client = CloudWatchClient::new(config.region.clone().try_into()?);

        let batch_size = config.batch_size.unwrap_or(bytesize::mib(1u64) as usize);
        let batch_timeout = config.batch_timeout.unwrap_or(1);

        let timeout = config.request_timeout_secs.unwrap_or(30);
        let in_flight_limit = config.request_in_flight_limit.unwrap_or(5);
        let rate_limit_duration = config.request_rate_limit_duration_secs.unwrap_or(1);
        let rate_limit_num = config.request_rate_limit_num.unwrap_or(5);
        let retry_attempts = config.request_retry_attempts.unwrap_or(usize::max_value());
        let retry_backoff_secs = config.request_retry_backoff_secs.unwrap_or(1);

        let policy = FixedRetryPolicy::new(
            retry_attempts,
            Duration::from_secs(retry_backoff_secs),
            CloudWatchMetricsRetryLogic,
        );

        let cloudwatch_metrics = CloudWatchMetricsService { client, config };

        let svc = ServiceBuilder::new()
            .concurrency_limit(in_flight_limit)
            .rate_limit(rate_limit_num, Duration::from_secs(rate_limit_duration))
            .retry(policy)
            .timeout(Duration::from_secs(timeout))
            .service(cloudwatch_metrics);

        let sink = BatchServiceSink::new(svc, acker).batched_with_min(
            Vec::new(),
            batch_size,
            Duration::from_secs(batch_timeout),
        );

        Ok(sink)
    }

    fn healthcheck(_config: &CloudWatchMetricsSinkConfig) -> Result<super::Healthcheck, String> {
        Ok(Box::new(future::ok(())))
    }
}

impl Service<Vec<Event>> for CloudWatchMetricsService {
    type Response = ();
    type Error = PutMetricDataError;
    type Future = Instrumented<RusotoFuture<(), PutMetricDataError>>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        Ok(().into())
    }

    fn call(&mut self, items: Vec<Event>) -> Self::Future {
        let input = encode_events(items).unwrap();

        debug!(message = "sending data.", input = &field::debug(&input));

        self.client
            .put_metric_data(input)
            .instrument(info_span!("request"))
    }
}

impl fmt::Debug for CloudWatchMetricsService {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("CloudWatchMetricsService")
            .field("config", &self.config)
            .finish()
    }
}

#[derive(Debug, Clone)]
struct CloudWatchMetricsRetryLogic;

impl RetryLogic for CloudWatchMetricsRetryLogic {
    type Error = PutMetricDataError;
    type Response = ();

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        match error {
            PutMetricDataError::HttpDispatch(_) => true,
            PutMetricDataError::InternalServiceFault(_) => true,
            PutMetricDataError::Unknown(res) if res.status.is_server_error() => true,
            _ => false,
        }
    }
}

fn encode_events(_events: Vec<Event>) -> Result<PutMetricDataInput, ()> {
    let datum = PutMetricDataInput {
        namespace: "namespace".to_string(),
        metric_data: Vec::new(),
    };

    Ok(datum)
}
