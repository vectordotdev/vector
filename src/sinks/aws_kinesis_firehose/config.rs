use aws_sdk_firehose::error::{
    DescribeDeliveryStreamError, PutRecordBatchError, PutRecordBatchErrorKind,
};
use aws_sdk_firehose::types::SdkError;
use aws_sdk_firehose::Client as KinesisFirehoseClient;
use futures::FutureExt;
use snafu::Snafu;
use tower::ServiceBuilder;
use vector_config::configurable_component;

use crate::{
    aws::{create_client, is_retriable_error, AwsAuthentication, ClientBuilder, RegionOrEndpoint},
    codecs::{Encoder, EncodingConfig},
    config::{
        AcknowledgementsConfig, DataType, GenerateConfig, Input, ProxyConfig, SinkConfig,
        SinkContext,
    },
    sinks::{
        aws_kinesis_firehose::{
            request_builder::KinesisRequestBuilder,
            service::{KinesisResponse, KinesisService},
            sink::KinesisSink,
        },
        util::{
            retries::RetryLogic, BatchConfig, Compression, ServiceBuilderExt, SinkBatchSettings,
            TowerRequestConfig,
        },
        Healthcheck, VectorSink,
    },
    tls::TlsConfig,
};

// AWS Kinesis Firehose API accepts payloads up to 4MB or 500 events
// https://docs.aws.amazon.com/firehose/latest/dev/limits.html
pub const MAX_PAYLOAD_SIZE: usize = 1024 * 1024 * 4;
pub const MAX_PAYLOAD_EVENTS: usize = 500;

#[derive(Clone, Copy, Debug, Default)]
pub struct KinesisFirehoseDefaultBatchSettings;

impl SinkBatchSettings for KinesisFirehoseDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = Some(MAX_PAYLOAD_EVENTS);
    const MAX_BYTES: Option<usize> = Some(MAX_PAYLOAD_SIZE);
    const TIMEOUT_SECS: f64 = 1.0;
}

/// Configuration for the `aws_kinesis_firehose` sink.
#[configurable_component(sink("aws_kinesis_firehose"))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct KinesisFirehoseSinkConfig {
    /// The [stream name][stream_name] of the target Kinesis Firehose delivery stream.
    ///
    /// [stream_name]: https://docs.aws.amazon.com/AmazonCloudWatch/latest/logs/Working-with-log-groups-and-streams.html
    pub stream_name: String,

    #[serde(flatten)]
    pub region: RegionOrEndpoint,

    #[configurable(derived)]
    pub encoding: EncodingConfig,

    #[configurable(derived)]
    #[serde(default)]
    pub compression: Compression,

    #[configurable(derived)]
    #[serde(default)]
    pub batch: BatchConfig<KinesisFirehoseDefaultBatchSettings>,

    #[configurable(derived)]
    #[serde(default)]
    pub request: TowerRequestConfig,

    #[configurable(derived)]
    pub tls: Option<TlsConfig>,

    #[configurable(derived)]
    #[serde(default)]
    pub auth: AwsAuthentication,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
}

#[derive(Debug, PartialEq, Eq, Snafu)]
pub enum BuildError {
    #[snafu(display(
        "Batch max size is too high. The value must be {} bytes or less",
        MAX_PAYLOAD_SIZE
    ))]
    BatchMaxSize,
    #[snafu(display(
        "Batch max events is too high. The value must be {} or less",
        MAX_PAYLOAD_EVENTS
    ))]
    BatchMaxEvents,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Snafu)]
enum HealthcheckError {
    #[snafu(display("DescribeDeliveryStream failed: {}", source))]
    DescribeDeliveryStreamFailed {
        source: SdkError<DescribeDeliveryStreamError>,
    },
    #[snafu(display("Stream name does not match, got {}, expected {}", name, stream_name))]
    StreamNamesMismatch { name: String, stream_name: String },
}

impl GenerateConfig for KinesisFirehoseSinkConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"region = "us-east-1"
            stream_name = "my-stream"
            encoding.codec = "json""#,
        )
        .unwrap()
    }
}

pub struct KinesisFirehoseClientBuilder;

impl ClientBuilder for KinesisFirehoseClientBuilder {
    type Config = aws_sdk_firehose::config::Config;
    type Client = aws_sdk_firehose::client::Client;
    type DefaultMiddleware = aws_sdk_firehose::middleware::DefaultMiddleware;

    fn default_middleware() -> Self::DefaultMiddleware {
        aws_sdk_firehose::middleware::DefaultMiddleware::new()
    }

    fn build(client: aws_smithy_client::Client, config: &aws_types::SdkConfig) -> Self::Client {
        aws_sdk_firehose::client::Client::with_config(client, config.into())
    }
}

#[async_trait::async_trait]
impl SinkConfig for KinesisFirehoseSinkConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let client = self.create_client(&cx.proxy).await?;
        let healthcheck = self.clone().healthcheck(client.clone()).boxed();

        let batch_settings = self
            .batch
            .validate()?
            .limit_max_bytes(MAX_PAYLOAD_SIZE)?
            .limit_max_events(MAX_PAYLOAD_EVENTS)?
            .into_batcher_settings()?;

        let request_limits = self.request.unwrap_with(&TowerRequestConfig::default());

        let region = self.region.region();
        let service = ServiceBuilder::new()
            .settings(request_limits, KinesisRetryLogic)
            .service(KinesisService {
                client,
                region,
                stream_name: self.stream_name.clone(),
            });

        let transformer = self.encoding.transformer();
        let serializer = self.encoding.build()?;
        let encoder = Encoder::<()>::new(serializer);

        let request_builder = KinesisRequestBuilder {
            compression: self.compression,
            encoder: (transformer, encoder),
        };

        let sink = KinesisSink {
            batch_settings,

            service,
            request_builder,
        };
        Ok((VectorSink::from_event_streamsink(sink), healthcheck))
    }

    fn input(&self) -> Input {
        Input::new(self.encoding.config().input_type() & DataType::Log)
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

impl KinesisFirehoseSinkConfig {
    async fn healthcheck(self, client: KinesisFirehoseClient) -> crate::Result<()> {
        let stream_name = self.stream_name;

        let result = client
            .describe_delivery_stream()
            .delivery_stream_name(stream_name.clone())
            .set_exclusive_start_destination_id(None)
            .limit(1)
            .send()
            .await;

        match result {
            Ok(resp) => {
                let name = resp
                    .delivery_stream_description
                    .and_then(|x| x.delivery_stream_name)
                    .unwrap_or_default();
                if name == stream_name {
                    Ok(())
                } else {
                    Err(HealthcheckError::StreamNamesMismatch { name, stream_name }.into())
                }
            }
            Err(source) => Err(HealthcheckError::DescribeDeliveryStreamFailed { source }.into()),
        }
    }

    pub async fn create_client(&self, proxy: &ProxyConfig) -> crate::Result<KinesisFirehoseClient> {
        create_client::<KinesisFirehoseClientBuilder>(
            &self.auth,
            self.region.region(),
            self.region.endpoint()?,
            proxy,
            &self.tls,
            true,
        )
        .await
    }
}

#[derive(Clone)]
pub struct KinesisRetryLogic;

impl RetryLogic for KinesisRetryLogic {
    type Error = SdkError<PutRecordBatchError>;
    type Response = KinesisResponse;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        if let SdkError::ServiceError { err, raw: _ } = error {
            if let PutRecordBatchErrorKind::ServiceUnavailableException(_) = err.kind {
                return true;
            }
        }
        is_retriable_error(error)
    }
}
