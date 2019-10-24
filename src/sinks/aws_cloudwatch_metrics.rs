use crate::{
    buffers::Acker,
    event::Metric,
    region::RegionOrEndpoint,
    sinks::util::{
        retries::{FixedRetryPolicy, RetryLogic},
        BatchServiceSink, MetricBuffer, SinkExt,
    },
    topology::config::{DataType, SinkConfig},
};
use chrono::{DateTime, SecondsFormat, Utc};
use futures::{Future, Poll};
use rusoto_cloudwatch::{
    CloudWatch, CloudWatchClient, Dimension, MetricDatum, PutMetricDataError, PutMetricDataInput,
};
use rusoto_core::{Region, RusotoFuture};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::{convert::TryInto, time::Duration};
use tower::{Service, ServiceBuilder};

#[derive(Clone)]
pub struct CloudWatchMetricsSvc {
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
    fn build(&self, acker: Acker) -> crate::Result<(super::RouterSink, super::Healthcheck)> {
        let sink = CloudWatchMetricsSvc::new(self.clone(), acker)?;
        let healthcheck = CloudWatchMetricsSvc::healthcheck(self)?;
        Ok((sink, healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Metric
    }
}

impl CloudWatchMetricsSvc {
    pub fn new(
        config: CloudWatchMetricsSinkConfig,
        acker: Acker,
    ) -> crate::Result<super::RouterSink> {
        let client = Self::create_client(config.region.clone().try_into()?)?;

        let batch_size = config.batch_size.unwrap_or(20);
        let batch_timeout = config.batch_timeout.unwrap_or(1);

        let timeout = config.request_timeout_secs.unwrap_or(30);
        let in_flight_limit = config.request_in_flight_limit.unwrap_or(5);
        let rate_limit_duration = config.request_rate_limit_duration_secs.unwrap_or(1);
        let rate_limit_num = config.request_rate_limit_num.unwrap_or(150);
        let retry_attempts = config.request_retry_attempts.unwrap_or(usize::max_value());
        let retry_backoff_secs = config.request_retry_backoff_secs.unwrap_or(1);

        let policy = FixedRetryPolicy::new(
            retry_attempts,
            Duration::from_secs(retry_backoff_secs),
            CloudWatchMetricsRetryLogic,
        );

        let cloudwatch_metrics = CloudWatchMetricsSvc { client, config };

        let svc = ServiceBuilder::new()
            .concurrency_limit(in_flight_limit)
            .rate_limit(rate_limit_num, Duration::from_secs(rate_limit_duration))
            .retry(policy)
            .timeout(Duration::from_secs(timeout))
            .service(cloudwatch_metrics);

        let sink = BatchServiceSink::new(svc, acker).batched_with_max(
            MetricBuffer::new(),
            batch_size,
            Duration::from_secs(batch_timeout),
        );

        Ok(Box::new(sink))
    }

    fn healthcheck(config: &CloudWatchMetricsSinkConfig) -> crate::Result<super::Healthcheck> {
        let client = Self::create_client(config.region.clone().try_into()?)?;

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
        let healthcheck = response.map_err(|err| err.into());

        Ok(Box::new(healthcheck))
    }

    fn create_client(region: Region) -> crate::Result<CloudWatchClient> {
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

    fn encode_events(&mut self, events: Vec<Metric>) -> PutMetricDataInput {
        let metric_data: Vec<_> = events
            .into_iter()
            .filter_map(|event| match event {
                Metric::Counter {
                    name,
                    val,
                    timestamp,
                    tags,
                } => Some(MetricDatum {
                    metric_name: name.to_string(),
                    value: Some(val),
                    timestamp: timestamp.map(timestamp_to_string),
                    dimensions: tags.map(tags_to_dimensions),
                    ..Default::default()
                }),
                Metric::Gauge {
                    name,
                    val,
                    direction: None,
                    timestamp,
                    tags,
                } => Some(MetricDatum {
                    metric_name: name.to_string(),
                    value: Some(val),
                    timestamp: timestamp.map(timestamp_to_string),
                    dimensions: tags.map(tags_to_dimensions),
                    ..Default::default()
                }),
                Metric::Histogram {
                    name,
                    val,
                    sample_rate,
                    timestamp,
                    tags,
                } => Some(MetricDatum {
                    metric_name: name.to_string(),
                    values: Some(vec![val]),
                    counts: Some(vec![f64::from(sample_rate)]),
                    timestamp: timestamp.map(timestamp_to_string),
                    dimensions: tags.map(tags_to_dimensions),
                    ..Default::default()
                }),
                _ => None,
            })
            .collect();

        let namespace = self.config.namespace.clone();

        PutMetricDataInput {
            namespace,
            metric_data,
        }
    }
}

impl Service<Vec<Metric>> for CloudWatchMetricsSvc {
    type Response = ();
    type Error = PutMetricDataError;
    type Future = RusotoFuture<(), PutMetricDataError>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        Ok(().into())
    }

    fn call(&mut self, items: Vec<Metric>) -> Self::Future {
        let input = self.encode_events(items);

        if !input.metric_data.is_empty() {
            debug!(message = "sending data.", ?input);
            self.client.put_metric_data(input)
        } else {
            Ok(()).into()
        }
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
            PutMetricDataError::Unknown(res)
                if res.status.is_server_error()
                    || res.status == http::StatusCode::TOO_MANY_REQUESTS =>
            {
                true
            }
            _ => false,
        }
    }
}

fn timestamp_to_string(timestamp: DateTime<Utc>) -> String {
    timestamp.to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn tags_to_dimensions(tags: HashMap<String, String>) -> Vec<Dimension> {
    // according to the API, up to 10 dimensions per metric can be provided
    tags.iter()
        .take(10)
        .map(|(k, v)| Dimension {
            name: k.to_string(),
            value: v.to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::metric::Metric;
    use chrono::offset::TimeZone;
    use pretty_assertions::assert_eq;
    use rusoto_cloudwatch::PutMetricDataInput;

    fn config() -> CloudWatchMetricsSinkConfig {
        CloudWatchMetricsSinkConfig {
            namespace: "vector".into(),
            region: RegionOrEndpoint::with_endpoint("local".to_owned()),
            ..Default::default()
        }
    }

    fn svc() -> CloudWatchMetricsSvc {
        let config = config();
        let region = config.region.clone().try_into().unwrap();
        let client = CloudWatchMetricsSvc::create_client(region).unwrap();

        CloudWatchMetricsSvc { client, config }
    }

    #[test]
    fn encode_events_basic_counter() {
        let events = vec![
            Metric::Counter {
                name: "exception_total".into(),
                val: 1.0,
                timestamp: None,
                tags: None,
            },
            Metric::Counter {
                name: "bytes_out".into(),
                val: 2.5,
                timestamp: Some(Utc.ymd(2018, 11, 14).and_hms_nano(8, 9, 10, 123456789)),
                tags: None,
            },
            Metric::Counter {
                name: "healthcheck".into(),
                val: 1.0,
                timestamp: Some(Utc.ymd(2018, 11, 14).and_hms_nano(8, 9, 10, 123456789)),
                tags: Some(
                    vec![("region".to_owned(), "local".to_owned())]
                        .into_iter()
                        .collect(),
                ),
            },
        ];

        assert_eq!(
            svc().encode_events(events),
            PutMetricDataInput {
                namespace: "vector".into(),
                metric_data: vec![
                    MetricDatum {
                        metric_name: "exception_total".into(),
                        value: Some(1.0),
                        ..Default::default()
                    },
                    MetricDatum {
                        metric_name: "bytes_out".into(),
                        value: Some(2.5),
                        timestamp: Some("2018-11-14T08:09:10.123Z".into()),
                        ..Default::default()
                    },
                    MetricDatum {
                        metric_name: "healthcheck".into(),
                        value: Some(1.0),
                        timestamp: Some("2018-11-14T08:09:10.123Z".into()),
                        dimensions: Some(vec![Dimension {
                            name: "region".into(),
                            value: "local".into()
                        }]),
                        ..Default::default()
                    },
                ],
            }
        );
    }

    #[test]
    fn encode_events_absolute_gauge() {
        let events = vec![Metric::Gauge {
            name: "temperature".into(),
            val: 10.0,
            direction: None,
            timestamp: None,
            tags: None,
        }];

        assert_eq!(
            svc().encode_events(events),
            PutMetricDataInput {
                namespace: "vector".into(),
                metric_data: vec![MetricDatum {
                    metric_name: "temperature".into(),
                    value: Some(10.0),
                    ..Default::default()
                }],
            }
        );
    }

    #[test]
    fn encode_events_histogram() {
        let events = vec![Metric::Histogram {
            name: "latency".into(),
            val: 11.0,
            sample_rate: 100,
            timestamp: None,
            tags: None,
        }];

        assert_eq!(
            svc().encode_events(events),
            PutMetricDataInput {
                namespace: "vector".into(),
                metric_data: vec![MetricDatum {
                    metric_name: "latency".into(),
                    values: Some(vec![11.0]),
                    counts: Some(vec![100.0]),
                    ..Default::default()
                }],
            }
        );
    }
}

#[cfg(feature = "cloudwatch-metrics-integration-tests")]
#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::event::Event;
    use crate::region::RegionOrEndpoint;
    use crate::test_util::{random_string, runtime};
    use chrono::offset::TimeZone;
    use futures::{stream, Sink};

    fn config() -> CloudWatchMetricsSinkConfig {
        CloudWatchMetricsSinkConfig {
            namespace: "vector".into(),
            region: RegionOrEndpoint::with_endpoint("http://localhost:4582".to_owned()),
            ..Default::default()
        }
    }

    #[test]
    fn cloudwatch_metrics_healthchecks() {
        let mut rt = runtime();

        let healthcheck = CloudWatchMetricsSvc::healthcheck(&config()).unwrap();
        rt.block_on(healthcheck).unwrap();
    }

    #[test]
    fn cloudwatch_metrics_put_data() {
        let mut rt = runtime();
        let sink = CloudWatchMetricsSvc::new(config(), Acker::Null).unwrap();

        let mut events = Vec::new();

        for i in 0..100 {
            let event = Event::Metric(Metric::Counter {
                name: format!("counter-{}", 0),
                val: i as f64,
                timestamp: None,
                tags: Some(
                    vec![
                        ("region".to_owned(), "us-west-1".to_owned()),
                        ("production".to_owned(), "true".to_owned()),
                        ("e".to_owned(), "".to_owned()),
                    ]
                    .into_iter()
                    .collect(),
                ),
            });
            events.push(event);
        }

        let gauge_name = random_string(10);
        for i in 0..10 {
            let event = Event::Metric(Metric::Gauge {
                name: format!("gauge-{}", gauge_name),
                val: i as f64,
                direction: None,
                timestamp: None,
                tags: None,
            });
            events.push(event);
        }

        let histogram_name = random_string(10);
        for i in 0..10 {
            let event = Event::Metric(Metric::Histogram {
                name: format!("histogram-{}", histogram_name),
                val: i as f64,
                sample_rate: 100,
                timestamp: Some(Utc.ymd(2018, 11, 14).and_hms_nano(8, 9, 10, 123456789)),
                tags: None,
            });
            events.push(event);
        }

        let stream = stream::iter_ok(events.clone().into_iter());

        let pump = sink.send_all(stream);
        let _ = rt.block_on(pump).unwrap();
    }
}
