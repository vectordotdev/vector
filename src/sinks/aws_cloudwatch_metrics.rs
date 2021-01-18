use crate::{
    config::{DataType, SinkConfig, SinkContext, SinkDescription},
    event::{
        metric::{Metric, MetricKind, MetricValue},
        Event,
    },
    rusoto::{self, RegionOrEndpoint},
    sinks::util::{
        retries::RetryLogic, BatchConfig, BatchSettings, Compression, MetricBuffer,
        PartitionBatchSink, PartitionBuffer, PartitionInnerBuffer, TowerRequestConfig,
    },
};
use chrono::{DateTime, SecondsFormat, Utc};
use futures::{future, future::BoxFuture, stream, FutureExt, SinkExt, StreamExt};
use lazy_static::lazy_static;
use rusoto_cloudwatch::{
    CloudWatch, CloudWatchClient, Dimension, MetricDatum, PutMetricDataError, PutMetricDataInput,
};
use rusoto_core::{Region, RusotoError};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    convert::TryInto,
    task::{Context, Poll},
};
use tower::Service;

#[derive(Clone)]
pub struct CloudWatchMetricsSvc {
    client: CloudWatchClient,
    config: CloudWatchMetricsSinkConfig,
}

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct CloudWatchMetricsSinkConfig {
    #[serde(alias = "namespace")]
    pub default_namespace: String,
    #[serde(flatten)]
    pub region: RegionOrEndpoint,
    #[serde(default)]
    pub compression: Compression,
    #[serde(default)]
    pub batch: BatchConfig,
    #[serde(default)]
    pub request: TowerRequestConfig,
    pub assume_role: Option<String>,
}

lazy_static! {
    static ref REQUEST_DEFAULTS: TowerRequestConfig = TowerRequestConfig {
        timeout_secs: Some(30),
        rate_limit_num: Some(150),
        ..Default::default()
    };
}

inventory::submit! {
    SinkDescription::new::<CloudWatchMetricsSinkConfig>("aws_cloudwatch_metrics")
}

impl_generate_config_from_default!(CloudWatchMetricsSinkConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "aws_cloudwatch_metrics")]
impl SinkConfig for CloudWatchMetricsSinkConfig {
    async fn build(
        &self,
        cx: SinkContext,
    ) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
        let client = self.create_client()?;
        let healthcheck = self.clone().healthcheck(client.clone()).boxed();
        let sink = CloudWatchMetricsSvc::new(self.clone(), client, cx)?;
        Ok((sink, healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Metric
    }

    fn sink_type(&self) -> &'static str {
        "aws_cloudwatch_metrics"
    }
}

impl CloudWatchMetricsSinkConfig {
    async fn healthcheck(self, client: CloudWatchClient) -> crate::Result<()> {
        let datum = MetricDatum {
            metric_name: "healthcheck".into(),
            value: Some(1.0),
            ..Default::default()
        };
        let request = PutMetricDataInput {
            namespace: self.default_namespace.clone(),
            metric_data: vec![datum],
        };

        client.put_metric_data(request).await.map_err(Into::into)
    }

    fn create_client(&self) -> crate::Result<CloudWatchClient> {
        let region = (&self.region).try_into()?;
        let region = if cfg!(test) {
            // Moto (used for mocking AWS) doesn't recognize 'custom' as valid region name
            match region {
                Region::Custom { endpoint, .. } => Region::Custom {
                    name: "us-east-1".into(),
                    endpoint,
                },
                _ => panic!("Only Custom regions are supported for CloudWatchClient testing"),
            }
        } else {
            region
        };

        let client = rusoto::client()?;
        let creds = rusoto::AwsCredentialsProvider::new(&region, self.assume_role.clone())?;

        let client = rusoto_core::Client::new_with_encoding(creds, client, self.compression.into());
        Ok(CloudWatchClient::new_with_client(client, region))
    }
}

impl CloudWatchMetricsSvc {
    pub fn new(
        config: CloudWatchMetricsSinkConfig,
        client: CloudWatchClient,
        cx: SinkContext,
    ) -> crate::Result<super::VectorSink> {
        let default_namespace = config.default_namespace.clone();
        let batch = BatchSettings::default()
            .events(20)
            .timeout(1)
            .parse_config(config.batch)?;
        let request = config.request.unwrap_with(&REQUEST_DEFAULTS);

        let cloudwatch_metrics = CloudWatchMetricsSvc { client, config };

        let svc = request.service(CloudWatchMetricsRetryLogic, cloudwatch_metrics);

        let buffer = PartitionBuffer::new(MetricBuffer::new(batch.size));

        let sink = PartitionBatchSink::new(svc, buffer, batch.timeout, cx.acker())
            .sink_map_err(|error| error!(message = "Fatal CloudwatchMetrics sink error.", %error))
            .with_flat_map(move |mut event: Event| {
                let namespace = event
                    .as_mut_metric()
                    .series
                    .name
                    .namespace
                    .take()
                    .unwrap_or_else(|| default_namespace.clone());
                stream::iter(Some(PartitionInnerBuffer::new(event, namespace))).map(Ok)
            });

        Ok(super::VectorSink::Sink(Box::new(sink)))
    }

    fn encode_events(&mut self, events: Vec<Metric>) -> Vec<MetricDatum> {
        events
            .into_iter()
            .filter_map(|event| {
                let metric_name = event.name().to_string();
                let timestamp = event.data.timestamp.map(timestamp_to_string);
                let dimensions = event.series.tags.clone().map(tags_to_dimensions);
                match event.data.kind {
                    MetricKind::Incremental => match event.data.value {
                        MetricValue::Counter { value } => Some(MetricDatum {
                            metric_name,
                            value: Some(value),
                            timestamp,
                            dimensions,
                            ..Default::default()
                        }),
                        MetricValue::Distribution {
                            samples,
                            statistic: _,
                        } => Some(MetricDatum {
                            metric_name,
                            values: Some(samples.iter().map(|s| s.value).collect()),
                            counts: Some(samples.iter().map(|s| f64::from(s.rate)).collect()),
                            timestamp,
                            dimensions,
                            ..Default::default()
                        }),
                        MetricValue::Set { values } => Some(MetricDatum {
                            metric_name,
                            value: Some(values.len() as f64),
                            timestamp,
                            dimensions,
                            ..Default::default()
                        }),
                        _ => None,
                    },
                    MetricKind::Absolute => match event.data.value {
                        MetricValue::Gauge { value } => Some(MetricDatum {
                            metric_name,
                            value: Some(value),
                            timestamp,
                            dimensions,
                            ..Default::default()
                        }),
                        _ => None,
                    },
                }
            })
            .collect()
    }
}

impl Service<PartitionInnerBuffer<Vec<Metric>, String>> for CloudWatchMetricsSvc {
    type Response = ();
    type Error = RusotoError<PutMetricDataError>;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, items: PartitionInnerBuffer<Vec<Metric>, String>) -> Self::Future {
        let (items, namespace) = items.into_parts();
        let metric_data = self.encode_events(items);
        if metric_data.is_empty() {
            return future::ok(()).boxed();
        }

        let input = PutMetricDataInput {
            namespace,
            metric_data,
        };

        debug!(message = "Sending data.", input = ?input);
        let client = self.client.clone();
        Box::pin(async move { client.put_metric_data(input).await })
    }
}

#[derive(Debug, Clone)]
struct CloudWatchMetricsRetryLogic;

impl RetryLogic for CloudWatchMetricsRetryLogic {
    type Error = RusotoError<PutMetricDataError>;
    type Response = ();

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        match error {
            RusotoError::Service(PutMetricDataError::InternalServiceFault(_)) => true,
            error => rusoto::is_retriable_error(error),
        }
    }
}

fn timestamp_to_string(timestamp: DateTime<Utc>) -> String {
    timestamp.to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn tags_to_dimensions(tags: BTreeMap<String, String>) -> Vec<Dimension> {
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
    use crate::event::metric::{Metric, MetricKind, MetricValue, StatisticKind};
    use chrono::offset::TimeZone;
    use pretty_assertions::assert_eq;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<CloudWatchMetricsSinkConfig>();
    }

    fn config() -> CloudWatchMetricsSinkConfig {
        CloudWatchMetricsSinkConfig {
            default_namespace: "vector".into(),
            region: RegionOrEndpoint::with_endpoint("local".to_owned()),
            ..Default::default()
        }
    }

    fn svc() -> CloudWatchMetricsSvc {
        let config = config();
        let client = config.create_client().unwrap();
        CloudWatchMetricsSvc { client, config }
    }

    #[test]
    fn encode_events_basic_counter() {
        let events = vec![
            Metric::new(
                "exception_total".into(),
                None,
                None,
                None,
                MetricKind::Incremental,
                MetricValue::Counter { value: 1.0 },
            ),
            Metric::new(
                "bytes_out".into(),
                None,
                Some(Utc.ymd(2018, 11, 14).and_hms_nano(8, 9, 10, 123456789)),
                None,
                MetricKind::Incremental,
                MetricValue::Counter { value: 2.5 },
            ),
            Metric::new(
                "healthcheck".into(),
                None,
                Some(Utc.ymd(2018, 11, 14).and_hms_nano(8, 9, 10, 123456789)),
                Some(
                    vec![("region".to_owned(), "local".to_owned())]
                        .into_iter()
                        .collect(),
                ),
                MetricKind::Incremental,
                MetricValue::Counter { value: 1.0 },
            ),
        ];

        assert_eq!(
            svc().encode_events(events),
            vec![
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
            ]
        );
    }

    #[test]
    fn encode_events_absolute_gauge() {
        let events = vec![Metric::new(
            "temperature".into(),
            None,
            None,
            None,
            MetricKind::Absolute,
            MetricValue::Gauge { value: 10.0 },
        )];

        assert_eq!(
            svc().encode_events(events),
            vec![MetricDatum {
                metric_name: "temperature".into(),
                value: Some(10.0),
                ..Default::default()
            }]
        );
    }

    #[test]
    fn encode_events_distribution() {
        let events = vec![Metric::new(
            "latency".into(),
            None,
            None,
            None,
            MetricKind::Incremental,
            MetricValue::Distribution {
                samples: crate::samples![11.0 => 100, 12.0 => 50],
                statistic: StatisticKind::Histogram,
            },
        )];

        assert_eq!(
            svc().encode_events(events),
            vec![MetricDatum {
                metric_name: "latency".into(),
                values: Some(vec![11.0, 12.0]),
                counts: Some(vec![100.0, 50.0]),
                ..Default::default()
            }]
        );
    }

    #[test]
    fn encode_events_set() {
        let events = vec![Metric::new(
            "users".into(),
            None,
            None,
            None,
            MetricKind::Incremental,
            MetricValue::Set {
                values: vec!["alice".into(), "bob".into()].into_iter().collect(),
            },
        )];

        assert_eq!(
            svc().encode_events(events),
            vec![MetricDatum {
                metric_name: "users".into(),
                value: Some(2.0),
                ..Default::default()
            }]
        );
    }
}

#[cfg(feature = "aws-cloudwatch-metrics-integration-tests")]
#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::{event::metric::StatisticKind, test_util::random_string, Event};
    use chrono::offset::TimeZone;
    use rand::seq::SliceRandom;

    fn config() -> CloudWatchMetricsSinkConfig {
        CloudWatchMetricsSinkConfig {
            default_namespace: "vector".into(),
            region: RegionOrEndpoint::with_endpoint("http://localhost:4566".to_owned()),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn cloudwatch_metrics_healthchecks() {
        let config = config();
        let client = config.create_client().unwrap();
        config.healthcheck(client).await.unwrap();
    }

    #[tokio::test]
    async fn cloudwatch_metrics_put_data() {
        let cx = SinkContext::new_test();
        let config = config();
        let client = config.create_client().unwrap();
        let sink = CloudWatchMetricsSvc::new(config, client, cx).unwrap();

        let mut events = Vec::new();

        for i in 0..100 {
            let event = Event::Metric(Metric::new(
                format!("counter-{}", 0),
                None,
                None,
                Some(
                    vec![
                        ("region".to_owned(), "us-west-1".to_owned()),
                        ("production".to_owned(), "true".to_owned()),
                        ("e".to_owned(), "".to_owned()),
                    ]
                    .into_iter()
                    .collect(),
                ),
                MetricKind::Incremental,
                MetricValue::Counter { value: i as f64 },
            ));
            events.push(event);
        }

        let gauge_name = random_string(10);
        for i in 0..10 {
            let event = Event::Metric(Metric::new(
                format!("gauge-{}", gauge_name),
                None,
                None,
                None,
                MetricKind::Absolute,
                MetricValue::Gauge { value: i as f64 },
            ));
            events.push(event);
        }

        let distribution_name = random_string(10);
        for i in 0..10 {
            let event = Event::Metric(Metric::new(
                format!("distribution-{}", distribution_name),
                None,
                Some(Utc.ymd(2018, 11, 14).and_hms_nano(8, 9, 10, 123456789)),
                None,
                MetricKind::Incremental,
                MetricValue::Distribution {
                    samples: crate::samples![i as f64 => 100],
                    statistic: StatisticKind::Histogram,
                },
            ));
            events.push(event);
        }

        let stream = stream::iter(events).map(Into::into);
        sink.run(stream).await.unwrap();
    }

    #[tokio::test]
    async fn cloudwatch_metrics_namespace_partitioning() {
        let cx = SinkContext::new_test();
        let config = config();
        let client = config.create_client().unwrap();
        let sink = CloudWatchMetricsSvc::new(config, client, cx).unwrap();

        let mut events = Vec::new();

        for namespace in ["ns1", "ns2", "ns3", "ns4"].iter() {
            for _ in 0..100 {
                let event = Event::Metric(Metric::new(
                    "counter".to_string(),
                    Some(namespace.to_string()),
                    None,
                    None,
                    MetricKind::Incremental,
                    MetricValue::Counter { value: 1.0 },
                ));
                events.push(event);
            }
        }

        events.shuffle(&mut rand::thread_rng());

        let stream = stream::iter(events).map(Into::into);
        sink.run(stream).await.unwrap();
    }
}
