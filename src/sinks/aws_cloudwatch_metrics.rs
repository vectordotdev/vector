use std::{
    collections::BTreeMap,
    task::{Context, Poll},
};

use aws_sdk_cloudwatch::error::PutMetricDataError;
use aws_sdk_cloudwatch::model::{Dimension, MetricDatum};
use aws_sdk_cloudwatch::types::DateTime as AwsDateTime;
use aws_sdk_cloudwatch::types::SdkError;
use aws_sdk_cloudwatch::{Client as CloudwatchClient, Endpoint, Region};
use aws_smithy_client::erase::DynConnector;
use aws_types::credentials::SharedCredentialsProvider;
use futures::{future, future::BoxFuture, stream, FutureExt, SinkExt};
use serde::{Deserialize, Serialize};
use tower::Service;
use vector_core::ByteSizeOf;

use super::util::SinkBatchSettings;
use crate::aws::RegionOrEndpoint;
use crate::aws::{create_client, is_retriable_error, ClientBuilder};
use crate::{
    aws::auth::AwsAuthentication,
    config::{
        AcknowledgementsConfig, Input, ProxyConfig, SinkConfig, SinkContext, SinkDescription,
    },
    event::{
        metric::{Metric, MetricValue},
        Event,
    },
    sinks::util::{
        batch::BatchConfig,
        buffer::metrics::{MetricNormalize, MetricNormalizer, MetricSet, MetricsBuffer},
        retries::RetryLogic,
        Compression, EncodedEvent, PartitionBuffer, PartitionInnerBuffer, TowerRequestConfig,
    },
    tls::TlsConfig,
};

#[derive(Clone)]
pub struct CloudWatchMetricsSvc {
    client: CloudwatchClient,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CloudWatchMetricsDefaultBatchSettings;

impl SinkBatchSettings for CloudWatchMetricsDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = Some(20);
    const MAX_BYTES: Option<usize> = None;
    const TIMEOUT_SECS: f64 = 1.0;
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
    pub batch: BatchConfig<CloudWatchMetricsDefaultBatchSettings>,
    #[serde(default)]
    pub request: TowerRequestConfig,
    pub tls: Option<TlsConfig>,
    // Deprecated name. Moved to auth.
    assume_role: Option<String>,
    #[serde(default)]
    pub auth: AwsAuthentication,
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    acknowledgements: AcknowledgementsConfig,
}

inventory::submit! {
    SinkDescription::new::<CloudWatchMetricsSinkConfig>("aws_cloudwatch_metrics")
}

impl_generate_config_from_default!(CloudWatchMetricsSinkConfig);

struct CloudwatchMetricsClientBuilder;

impl ClientBuilder for CloudwatchMetricsClientBuilder {
    type ConfigBuilder = aws_sdk_cloudwatch::config::Builder;
    type Client = CloudwatchClient;

    fn create_config_builder(
        credentials_provider: SharedCredentialsProvider,
    ) -> Self::ConfigBuilder {
        aws_sdk_cloudwatch::config::Builder::new().credentials_provider(credentials_provider)
    }

    fn with_endpoint_resolver(
        builder: Self::ConfigBuilder,
        endpoint: Endpoint,
    ) -> Self::ConfigBuilder {
        builder.endpoint_resolver(endpoint)
    }

    fn with_region(builder: Self::ConfigBuilder, region: Region) -> Self::ConfigBuilder {
        builder.region(region)
    }

    fn client_from_conf_conn(
        builder: Self::ConfigBuilder,
        connector: DynConnector,
    ) -> Self::Client {
        Self::Client::from_conf_conn(builder.build(), connector)
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "aws_cloudwatch_metrics")]
impl SinkConfig for CloudWatchMetricsSinkConfig {
    async fn build(
        &self,
        cx: SinkContext,
    ) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
        let client = self.create_client(&cx.proxy).await?;
        let healthcheck = self.clone().healthcheck(client.clone()).boxed();
        let sink = CloudWatchMetricsSvc::new(self.clone(), client, cx)?;
        Ok((sink, healthcheck))
    }

    fn input(&self) -> Input {
        Input::metric()
    }

    fn sink_type(&self) -> &'static str {
        "aws_cloudwatch_metrics"
    }

    fn acknowledgements(&self) -> Option<&AcknowledgementsConfig> {
        Some(&self.acknowledgements)
    }
}

impl CloudWatchMetricsSinkConfig {
    async fn healthcheck(self, client: CloudwatchClient) -> crate::Result<()> {
        let _result = client
            .put_metric_data()
            .metric_data(
                MetricDatum::builder()
                    .metric_name("healthcheck")
                    .value(1.0)
                    .build(),
            )
            .namespace(&self.default_namespace)
            .send()
            .await?;

        Ok(())
    }

    async fn create_client(&self, proxy: &ProxyConfig) -> crate::Result<CloudwatchClient> {
        let region = if cfg!(test) {
            // Moto (used for mocking AWS) doesn't recognize 'custom' as valid region name
            Region::new("us-east-1")
        } else {
            self.region.region()
        };

        create_client::<CloudwatchMetricsClientBuilder>(
            &self.auth,
            region,
            self.region.endpoint()?,
            proxy,
            &self.tls,
        )
        .await
    }
}

impl CloudWatchMetricsSvc {
    pub fn new(
        config: CloudWatchMetricsSinkConfig,
        client: CloudwatchClient,
        cx: SinkContext,
    ) -> crate::Result<super::VectorSink> {
        let default_namespace = config.default_namespace.clone();
        let batch = config.batch.into_batch_settings()?;
        let request_settings = config.request.unwrap_with(&TowerRequestConfig {
            timeout_secs: Some(30),
            rate_limit_num: Some(150),
            ..Default::default()
        });

        let service = CloudWatchMetricsSvc { client };
        let buffer = PartitionBuffer::new(MetricsBuffer::new(batch.size));
        let mut normalizer = MetricNormalizer::<AwsCloudwatchMetricNormalize>::default();

        let sink = request_settings
            .partition_sink(
                CloudWatchMetricsRetryLogic,
                service,
                buffer,
                batch.timeout,
                cx.acker(),
            )
            .sink_map_err(|error| error!(message = "Fatal CloudwatchMetrics sink error.", %error))
            .with_flat_map(move |event: Event| {
                stream::iter({
                    let byte_size = event.size_of();
                    normalizer.normalize(event.into_metric()).map(|mut metric| {
                        let namespace = metric
                            .take_namespace()
                            .take()
                            .unwrap_or_else(|| default_namespace.clone());
                        Ok(EncodedEvent::new(
                            PartitionInnerBuffer::new(metric, namespace),
                            byte_size,
                        ))
                    })
                })
            });

        Ok(super::VectorSink::from_event_sink(sink))
    }

    fn encode_events(&mut self, events: Vec<Metric>) -> Vec<MetricDatum> {
        events
            .into_iter()
            .filter_map(|event| {
                let metric_name = event.name().to_string();
                // let timestamp = event.timestamp().map(timestamp_to_string);
                let timestamp = event
                    .timestamp()
                    .map(|x| AwsDateTime::from_millis(x.timestamp_millis()));
                let dimensions = event.tags().map(tags_to_dimensions);
                // AwsCloudwatchMetricNormalize converts these to the right MetricKind
                match event.value() {
                    MetricValue::Counter { value } => Some(
                        MetricDatum::builder()
                            .metric_name(metric_name)
                            .value(*value)
                            .set_timestamp(timestamp)
                            .set_dimensions(dimensions)
                            .build(),
                    ),
                    MetricValue::Distribution {
                        samples,
                        statistic: _,
                    } => Some(
                        MetricDatum::builder()
                            .metric_name(metric_name)
                            .set_values(Some(samples.iter().map(|s| s.value).collect()))
                            .set_counts(Some(samples.iter().map(|s| s.rate as f64).collect()))
                            .set_timestamp(timestamp)
                            .set_dimensions(dimensions)
                            .build(),
                    ),
                    MetricValue::Set { values } => Some(
                        MetricDatum::builder()
                            .metric_name(metric_name)
                            .value(values.len() as f64)
                            .set_timestamp(timestamp)
                            .set_dimensions(dimensions)
                            .build(),
                    ),
                    MetricValue::Gauge { value } => Some(
                        MetricDatum::builder()
                            .metric_name(metric_name)
                            .value(*value)
                            .set_timestamp(timestamp)
                            .set_dimensions(dimensions)
                            .build(),
                    ),
                    _ => None,
                }
            })
            .collect()
    }
}

#[derive(Default)]
struct AwsCloudwatchMetricNormalize;

impl MetricNormalize for AwsCloudwatchMetricNormalize {
    fn normalize(&mut self, state: &mut MetricSet, metric: Metric) -> Option<Metric> {
        match metric.value() {
            MetricValue::Gauge { .. } => state.make_absolute(metric),
            _ => state.make_incremental(metric),
        }
    }
}

impl Service<PartitionInnerBuffer<Vec<Metric>, String>> for CloudWatchMetricsSvc {
    type Response = ();
    type Error = SdkError<PutMetricDataError>;
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

        let client = self.client.clone();

        Box::pin(async move {
            client
                .put_metric_data()
                .namespace(namespace)
                .set_metric_data(Some(metric_data))
                .send()
                .await?;
            Ok(())
        })
    }
}

#[derive(Debug, Clone)]
struct CloudWatchMetricsRetryLogic;

impl RetryLogic for CloudWatchMetricsRetryLogic {
    type Error = SdkError<PutMetricDataError>;
    type Response = ();

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        is_retriable_error(error)
    }
}

fn tags_to_dimensions(tags: &BTreeMap<String, String>) -> Vec<Dimension> {
    // according to the API, up to 10 dimensions per metric can be provided
    tags.iter()
        .take(10)
        .map(|(k, v)| Dimension::builder().name(k).value(v).build())
        .collect()
}

#[cfg(test)]
mod tests {
    use aws_sdk_cloudwatch::types::DateTime;
    use chrono::offset::TimeZone;
    use chrono::Utc;
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::event::metric::{Metric, MetricKind, MetricValue, StatisticKind};

    fn timestamp(time: &str) -> DateTime {
        DateTime::from_millis(
            chrono::DateTime::parse_from_rfc3339(time)
                .unwrap()
                .timestamp_millis(),
        )
    }

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<CloudWatchMetricsSinkConfig>();
    }

    fn config() -> CloudWatchMetricsSinkConfig {
        CloudWatchMetricsSinkConfig {
            default_namespace: "vector".into(),
            region: RegionOrEndpoint::with_region("local".to_owned()),
            ..Default::default()
        }
    }

    async fn svc() -> CloudWatchMetricsSvc {
        let config = config();
        let client = config
            .create_client(&ProxyConfig::from_env())
            .await
            .unwrap();
        CloudWatchMetricsSvc { client }
    }

    #[tokio::test]
    async fn encode_events_basic_counter() {
        let events = vec![
            Metric::new(
                "exception_total",
                MetricKind::Incremental,
                MetricValue::Counter { value: 1.0 },
            ),
            Metric::new(
                "bytes_out",
                MetricKind::Incremental,
                MetricValue::Counter { value: 2.5 },
            )
            .with_timestamp(Some(
                Utc.ymd(2018, 11, 14).and_hms_nano(8, 9, 10, 123456789),
            )),
            Metric::new(
                "healthcheck",
                MetricKind::Incremental,
                MetricValue::Counter { value: 1.0 },
            )
            .with_tags(Some(
                vec![("region".to_owned(), "local".to_owned())]
                    .into_iter()
                    .collect(),
            ))
            .with_timestamp(Some(
                Utc.ymd(2018, 11, 14).and_hms_nano(8, 9, 10, 123456789),
            )),
        ];

        assert_eq!(
            svc().await.encode_events(events),
            vec![
                MetricDatum::builder()
                    .metric_name("exception_total")
                    .value(1.0)
                    .build(),
                MetricDatum::builder()
                    .metric_name("bytes_out")
                    .value(2.5)
                    .timestamp(timestamp("2018-11-14T08:09:10.123Z"))
                    .build(),
                MetricDatum::builder()
                    .metric_name("healthcheck")
                    .value(1.0)
                    .timestamp(timestamp("2018-11-14T08:09:10.123Z"))
                    .dimensions(Dimension::builder().name("region").value("local").build())
                    .build()
            ]
        );
    }

    #[tokio::test]
    async fn encode_events_absolute_gauge() {
        let events = vec![Metric::new(
            "temperature",
            MetricKind::Absolute,
            MetricValue::Gauge { value: 10.0 },
        )];

        assert_eq!(
            svc().await.encode_events(events),
            vec![MetricDatum::builder()
                .metric_name("temperature")
                .value(10.0)
                .build()]
        );
    }

    #[tokio::test]
    async fn encode_events_distribution() {
        let events = vec![Metric::new(
            "latency",
            MetricKind::Incremental,
            MetricValue::Distribution {
                samples: vector_core::samples![11.0 => 100, 12.0 => 50],
                statistic: StatisticKind::Histogram,
            },
        )];

        assert_eq!(
            svc().await.encode_events(events),
            vec![MetricDatum::builder()
                .metric_name("latency")
                .set_values(Some(vec![11.0, 12.0]))
                .set_counts(Some(vec![100.0, 50.0]))
                .build()]
        );
    }

    #[tokio::test]
    async fn encode_events_set() {
        let events = vec![Metric::new(
            "users",
            MetricKind::Incremental,
            MetricValue::Set {
                values: vec!["alice".into(), "bob".into()].into_iter().collect(),
            },
        )];

        assert_eq!(
            svc().await.encode_events(events),
            vec![MetricDatum::builder()
                .metric_name("users")
                .value(2.0)
                .build()]
        );
    }
}

#[cfg(feature = "aws-cloudwatch-metrics-integration-tests")]
#[cfg(test)]
mod integration_tests {
    use chrono::offset::TimeZone;
    use chrono::Utc;
    use futures::StreamExt;
    use rand::seq::SliceRandom;

    use super::*;
    use crate::{
        event::{metric::StatisticKind, Event, MetricKind},
        test_util::random_string,
    };

    fn cloudwatch_address() -> String {
        std::env::var("CLOUDWATCH_ADDRESS").unwrap_or_else(|_| "http://localhost:4566".into())
    }

    fn config() -> CloudWatchMetricsSinkConfig {
        CloudWatchMetricsSinkConfig {
            default_namespace: "vector".into(),
            region: RegionOrEndpoint::with_both("local", cloudwatch_address().as_str()),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn cloudwatch_metrics_healthchecks() {
        let config = config();
        let client = config
            .create_client(&ProxyConfig::from_env())
            .await
            .unwrap();
        config.healthcheck(client).await.unwrap();
    }

    #[tokio::test]
    async fn cloudwatch_metrics_put_data() {
        let cx = SinkContext::new_test();
        let config = config();
        let client = config.create_client(&cx.globals.proxy).await.unwrap();
        let sink = CloudWatchMetricsSvc::new(config, client, cx).unwrap();

        let mut events = Vec::new();

        for i in 0..100 {
            let event = Event::Metric(
                Metric::new(
                    format!("counter-{}", 0),
                    MetricKind::Incremental,
                    MetricValue::Counter { value: i as f64 },
                )
                .with_tags(Some(
                    vec![
                        ("region".to_owned(), "us-west-1".to_owned()),
                        ("production".to_owned(), "true".to_owned()),
                        ("e".to_owned(), "".to_owned()),
                    ]
                    .into_iter()
                    .collect(),
                )),
            );
            events.push(event);
        }

        let gauge_name = random_string(10);
        for i in 0..10 {
            let event = Event::Metric(Metric::new(
                format!("gauge-{}", gauge_name),
                MetricKind::Absolute,
                MetricValue::Gauge { value: i as f64 },
            ));
            events.push(event);
        }

        let distribution_name = random_string(10);
        for i in 0..10 {
            let event = Event::Metric(
                Metric::new(
                    format!("distribution-{}", distribution_name),
                    MetricKind::Incremental,
                    MetricValue::Distribution {
                        samples: vector_core::samples![i as f64 => 100],
                        statistic: StatisticKind::Histogram,
                    },
                )
                .with_timestamp(Some(
                    Utc.ymd(2018, 11, 14).and_hms_nano(8, 9, 10, 123456789),
                )),
            );
            events.push(event);
        }

        let stream = stream::iter(events).map(Into::into);
        sink.run(stream).await.unwrap();
    }

    #[tokio::test]
    async fn cloudwatch_metrics_namespace_partitioning() {
        let cx = SinkContext::new_test();
        let config = config();
        let client = config.create_client(&cx.globals.proxy).await.unwrap();
        let sink = CloudWatchMetricsSvc::new(config, client, cx).unwrap();

        let mut events = Vec::new();

        for namespace in ["ns1", "ns2", "ns3", "ns4"].iter() {
            for _ in 0..100 {
                let event = Event::Metric(
                    Metric::new(
                        "counter",
                        MetricKind::Incremental,
                        MetricValue::Counter { value: 1.0 },
                    )
                    .with_namespace(Some(*namespace)),
                );
                events.push(event);
            }
        }

        events.shuffle(&mut rand::thread_rng());

        let stream = stream::iter(events).map(Into::into);
        sink.run(stream).await.unwrap();
    }
}
