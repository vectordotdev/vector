use crate::{
    buffers::Acker,
    event::{Event, Metric},
    region::RegionOrEndpoint,
    sinks::util::{
        retries::{FixedRetryPolicy, RetryLogic},
        BatchServiceSink, Partition, SinkExt,
    },
    topology::config::{DataType, SinkConfig},
};
use futures::{Future, Poll};
use rusoto_cloudwatch::{
    CloudWatch, CloudWatchClient, MetricDatum, PutMetricDataError, PutMetricDataInput,
};
use rusoto_core::{Region, RusotoFuture};
use serde::{Deserialize, Serialize};
use std::{convert::TryInto, time::Duration};
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
        let sink = CloudWatchMetricsService::new(self.clone(), acker)?;
        let healthcheck = CloudWatchMetricsService::healthcheck(self)?;
        Ok((sink, healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Metric
    }
}

impl CloudWatchMetricsService {
    pub fn new(
        config: CloudWatchMetricsSinkConfig,
        acker: Acker,
    ) -> Result<super::RouterSink, String> {
        let client = CloudWatchClient::new(config.region.clone().try_into()?);

        let batch_size = config.batch_size.unwrap_or(5);
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

        let sink = BatchServiceSink::new(svc, acker).partitioned_batched_with_min(
            Vec::new(),
            batch_size,
            Duration::from_secs(batch_timeout),
        );

        Ok(Box::new(sink))
    }

    fn healthcheck(config: &CloudWatchMetricsSinkConfig) -> Result<super::Healthcheck, String> {
        let region = config.region.clone();
        let client = Self::create_client(region.try_into()?)?;

        let datum = MetricDatum {
            metric_name: "healthcheck".into(),
            value: Some(1.0),
            ..Default::default()
        };
        let request = PutMetricDataInput {
            namespace: config.namespace.clone(),
            metric_data: vec![datum],
        };

        let response = client.put_metric_data(request);
        let healthcheck = response.map_err(|err| err.to_string());

        Ok(Box::new(healthcheck))
    }

    fn create_client(region: Region) -> Result<CloudWatchClient, String> {
        #[cfg(test)]
        {
            // Moto (used for mocking AWS) doesn't recognize 'custom' as valid region name
            let region = match region {
                Region::Custom { endpoint, .. } => Region::Custom {
                    name: "us-east-1".into(),
                    endpoint,
                },
                _ => panic!("Only Custom regions are supported for CloudWatchClient testing"),
            };
            Ok(CloudWatchClient::new(region))
        }

        #[cfg(not(test))]
        {
            Ok(CloudWatchClient::new(region))
        }
    }
}

use bytes::Bytes;

impl Partition<Bytes> for Event {
    fn partition(&self) -> Bytes {
        "key".into()
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
        let namespace = self.config.namespace.clone();
        let input = encode_events(items, namespace).unwrap();

        debug!(message = "sending data.", input = &field::debug(&input));

        self.client
            .put_metric_data(input)
            .instrument(info_span!("request"))
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

fn encode_events(events: Vec<Event>, namespace: String) -> Result<PutMetricDataInput, ()> {
    let metric_data: Vec<_> = events
        .iter()
        .filter_map(|event| match event.as_metric() {
            Metric::Counter { name, val } => Some(MetricDatum {
                metric_name: name.to_string(),
                value: Some(*val as f64),
                ..Default::default()
            }),
            _ => None,
        })
        .collect();

    let datum = PutMetricDataInput {
        namespace,
        metric_data,
    };

    Ok(datum)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{event::metric::Metric, Event};
    use rusoto_cloudwatch::PutMetricDataInput;

    #[test]
    fn encode_events_basic_counter() {
        let namespace = String::from("namespace");

        let events = vec![
            Event::Metric(Metric::Counter {
                name: "exception_total".into(),
                val: 1.0,
            }),
            Event::Metric(Metric::Counter {
                name: "bytes_out".into(),
                val: 2.5,
            }),
        ];

        assert_eq!(
            encode_events(events, namespace.clone()).unwrap(),
            PutMetricDataInput {
                namespace,
                metric_data: vec![
                    MetricDatum {
                        metric_name: "exception_total".into(),
                        value: Some(1.0),
                        ..Default::default()
                    },
                    MetricDatum {
                        metric_name: "bytes_out".into(),
                        value: Some(2.5),
                        ..Default::default()
                    }
                ],
            }
        );
    }
}

#[cfg(feature = "cloudwatch-metrics-integration-tests")]
#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::region::RegionOrEndpoint;
    use crate::test_util::random_string;
    use futures::{stream, Sink};
    use tokio::runtime::Runtime;

    fn config() -> CloudWatchMetricsSinkConfig {
        CloudWatchMetricsSinkConfig {
            namespace: "vector".into(),
            region: RegionOrEndpoint::with_endpoint("http://localhost:4582".to_owned()),
            ..Default::default()
        }
    }

    #[test]
    fn cloudwatch_metrics_healthchecks() {
        let mut rt = tokio::runtime::Runtime::new().unwrap();

        let healthcheck = CloudWatchMetricsService::healthcheck(&config()).unwrap();
        rt.block_on(healthcheck).unwrap();
    }

    #[test]
    fn cloudwatch_metrics_put_data() {
        let mut rt = Runtime::new().unwrap();
        let sink = CloudWatchMetricsService::new(config(), Acker::Null).unwrap();

        let mut events = Vec::new();
        let name = random_string(10);
        for i in 0..10 {
            let event = Event::Metric(Metric::Counter {
                name: name.clone(),
                val: i as f32,
            });
            events.push(event);
        }

        let stream = stream::iter_ok(events.clone().into_iter());

        let pump = sink.send_all(stream);
        rt.block_on(pump).unwrap();
    }
}
