#[cfg(all(test, feature = "aws-cloudwatch-metrics-integration-tests"))]
mod integration_tests;
#[cfg(test)]
mod tests;

use aws_config::Region;
use aws_sdk_cloudwatch::error::SdkError;
use aws_sdk_cloudwatch::operation::put_metric_data::PutMetricDataError;
use aws_sdk_cloudwatch::types::{Dimension, MetricDatum};
use aws_sdk_cloudwatch::Client as CloudwatchClient;
use aws_smithy_types::DateTime as AwsDateTime;
use futures::{stream, FutureExt, SinkExt};
use futures_util::{future, future::BoxFuture};
use std::task::{Context, Poll};
use tower::Service;
use vector_lib::configurable::configurable_component;
use vector_lib::{sink::VectorSink, ByteSizeOf, EstimatedJsonEncodedSizeOf};

use crate::{
    aws::{
        auth::AwsAuthentication, create_client, is_retriable_error, ClientBuilder, RegionOrEndpoint,
    },
    config::{AcknowledgementsConfig, Input, ProxyConfig, SinkConfig, SinkContext},
    event::{
        metric::{Metric, MetricTags, MetricValue},
        Event,
    },
    sinks::util::{
        batch::BatchConfig,
        buffer::metrics::{MetricNormalize, MetricNormalizer, MetricSet, MetricsBuffer},
        retries::RetryLogic,
        Compression, EncodedEvent, PartitionBuffer, PartitionInnerBuffer, SinkBatchSettings,
        TowerRequestConfig,
    },
    tls::TlsConfig,
};

use super::util::service::TowerRequestConfigDefaults;

#[derive(Clone, Copy, Debug, Default)]
pub struct CloudWatchMetricsDefaultBatchSettings;

impl SinkBatchSettings for CloudWatchMetricsDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = Some(20);
    const MAX_BYTES: Option<usize> = None;
    const TIMEOUT_SECS: f64 = 1.0;
}

#[derive(Clone, Copy, Debug)]
pub struct CloudWatchMetricsTowerRequestConfigDefaults;

impl TowerRequestConfigDefaults for CloudWatchMetricsTowerRequestConfigDefaults {
    const RATE_LIMIT_NUM: u64 = 150;
}

/// Configuration for the `aws_cloudwatch_metrics` sink.
#[configurable_component(sink(
    "aws_cloudwatch_metrics",
    "Publish metric events to AWS CloudWatch Metrics."
))]
#[derive(Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct CloudWatchMetricsSinkConfig {
    /// The default [namespace][namespace] to use for metrics that do not have one.
    ///
    /// Metrics with the same name can only be differentiated by their namespace, and not all
    /// metrics have their own namespace.
    ///
    /// [namespace]: https://docs.aws.amazon.com/AmazonCloudWatch/latest/monitoring/cloudwatch_concepts.html#Namespace
    #[serde(alias = "namespace")]
    #[configurable(metadata(docs::examples = "service"))]
    pub default_namespace: String,

    /// The [AWS region][aws_region] of the target service.
    ///
    /// [aws_region]: https://docs.aws.amazon.com/AmazonRDS/latest/UserGuide/Concepts.RegionsAndAvailabilityZones.html
    #[serde(flatten)]
    pub region: RegionOrEndpoint,

    #[configurable(derived)]
    #[serde(default)]
    pub compression: Compression,

    #[configurable(derived)]
    #[serde(default)]
    pub batch: BatchConfig<CloudWatchMetricsDefaultBatchSettings>,

    #[configurable(derived)]
    #[serde(default)]
    pub request: TowerRequestConfig<CloudWatchMetricsTowerRequestConfigDefaults>,

    #[configurable(derived)]
    pub tls: Option<TlsConfig>,

    /// The ARN of an [IAM role][iam_role] to assume at startup.
    ///
    /// [iam_role]: https://docs.aws.amazon.com/IAM/latest/UserGuide/id_roles.html
    #[configurable(deprecated)]
    #[configurable(metadata(docs::hidden))]
    assume_role: Option<String>,

    #[configurable(derived)]
    #[serde(default)]
    pub auth: AwsAuthentication,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    acknowledgements: AcknowledgementsConfig,
}

impl_generate_config_from_default!(CloudWatchMetricsSinkConfig);

struct CloudwatchMetricsClientBuilder;

impl ClientBuilder for CloudwatchMetricsClientBuilder {
    type Client = aws_sdk_cloudwatch::client::Client;

    fn build(config: &aws_types::SdkConfig) -> Self::Client {
        aws_sdk_cloudwatch::client::Client::new(config)
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
        let sink = CloudWatchMetricsSvc::new(self.clone(), client)?;
        Ok((sink, healthcheck))
    }

    fn input(&self) -> Input {
        Input::metric()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

impl CloudWatchMetricsSinkConfig {
    async fn healthcheck(self, client: CloudwatchClient) -> crate::Result<()> {
        client
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
            Some(Region::new("us-east-1"))
        } else {
            self.region.region()
        };

        create_client::<CloudwatchMetricsClientBuilder>(
            &self.auth,
            region,
            self.region.endpoint(),
            proxy,
            &self.tls,
        )
        .await
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

#[derive(Debug, Clone)]
struct CloudWatchMetricsRetryLogic;

impl RetryLogic for CloudWatchMetricsRetryLogic {
    type Error = SdkError<PutMetricDataError>;
    type Response = ();

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        is_retriable_error(error)
    }
}

fn tags_to_dimensions(tags: &MetricTags) -> Vec<Dimension> {
    // according to the API, up to 30 dimensions per metric can be provided
    tags.iter_single()
        .take(30)
        .map(|(k, v)| Dimension::builder().name(k).value(v).build())
        .collect()
}

#[derive(Clone)]
pub struct CloudWatchMetricsSvc {
    client: CloudwatchClient,
}

impl CloudWatchMetricsSvc {
    pub fn new(
        config: CloudWatchMetricsSinkConfig,
        client: CloudwatchClient,
    ) -> crate::Result<VectorSink> {
        let default_namespace = config.default_namespace.clone();
        let batch = config.batch.into_batch_settings()?;
        let request_settings = config.request.into_settings();

        let service = CloudWatchMetricsSvc { client };
        let buffer = PartitionBuffer::new(MetricsBuffer::new(batch.size));
        let mut normalizer = MetricNormalizer::<AwsCloudwatchMetricNormalize>::default();

        let sink = request_settings
            .partition_sink(CloudWatchMetricsRetryLogic, service, buffer, batch.timeout)
            .sink_map_err(|error| error!(message = "Fatal CloudwatchMetrics sink error.", %error))
            .with_flat_map(move |event: Event| {
                stream::iter({
                    let byte_size = event.allocated_bytes();
                    let json_byte_size = event.estimated_json_encoded_size_of();
                    normalizer.normalize(event.into_metric()).map(|mut metric| {
                        let namespace = metric
                            .take_namespace()
                            .take()
                            .unwrap_or_else(|| default_namespace.clone());
                        Ok(EncodedEvent::new(
                            PartitionInnerBuffer::new(metric, namespace),
                            byte_size,
                            json_byte_size,
                        ))
                    })
                })
            });

        #[allow(deprecated)]
        Ok(VectorSink::from_event_sink(sink))
    }

    fn encode_events(&mut self, events: Vec<Metric>) -> Vec<MetricDatum> {
        events
            .into_iter()
            .filter_map(|event| {
                let metric_name = event.name().to_string();
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

impl Service<PartitionInnerBuffer<Vec<Metric>, String>> for CloudWatchMetricsSvc {
    type Response = ();
    type Error = SdkError<PutMetricDataError>;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    // Emission of an internal event in case of errors is handled upstream by the caller.
    fn poll_ready(&mut self, _cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    // Emission of internal events for errors and dropped events is handled upstream by the caller.
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
