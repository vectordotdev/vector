use aws_sdk_firehose::{
    config::Config,
    error::{DescribeDeliveryStreamError, PutRecordBatchErrorKind},
    middleware::DefaultMiddleware,
    types::SdkError,
};
use futures::FutureExt;
use snafu::Snafu;
use vector_config::configurable_component;

use crate::{
    aws::{create_client, is_retriable_error, ClientBuilder},
    config::{AcknowledgementsConfig, GenerateConfig, Input, ProxyConfig, SinkConfig, SinkContext},
    sinks::{
        util::{retries::RetryLogic, BatchConfig, SinkBatchSettings},
        Healthcheck, VectorSink,
    },
};

use super::{
    build_sink,
    record::{KinesisFirehoseClient, KinesisFirehoseRecord},
    KinesisClient, KinesisError, KinesisRecord, KinesisResponse, KinesisSinkBaseConfig,
};

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

pub struct KinesisFirehoseClientBuilder;

impl ClientBuilder for KinesisFirehoseClientBuilder {
    type Config = Config;
    type Client = KinesisClient;
    type DefaultMiddleware = DefaultMiddleware;

    fn default_middleware() -> Self::DefaultMiddleware {
        DefaultMiddleware::new()
    }

    fn build(client: aws_smithy_client::Client, config: &aws_types::SdkConfig) -> Self::Client {
        Self::Client::with_config(client, config.into())
    }
}

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
pub struct KinesisFirehoseSinkConfig {
    #[serde(flatten)]
    pub base: KinesisSinkBaseConfig,

    #[configurable(derived)]
    #[serde(default)]
    pub batch: BatchConfig<KinesisFirehoseDefaultBatchSettings>,
}

impl KinesisFirehoseSinkConfig {
    async fn healthcheck(self, client: KinesisClient) -> crate::Result<()> {
        let stream_name = self.base.stream_name;

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

    pub async fn create_client(&self, proxy: &ProxyConfig) -> crate::Result<KinesisClient> {
        create_client::<KinesisFirehoseClientBuilder>(
            &self.base.auth,
            self.base.region.region(),
            self.base.region.endpoint()?,
            proxy,
            &self.base.tls,
            true,
        )
        .await
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

        let sink = build_sink::<
            KinesisFirehoseClient,
            KinesisRecord,
            KinesisFirehoseRecord,
            KinesisError,
            KinesisRetryLogic,
        >(
            &self.base,
            None,
            batch_settings,
            KinesisFirehoseClient { client },
        )
        .await?;

        Ok((sink, healthcheck))
    }

    fn input(&self) -> Input {
        self.base.input()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        self.base.acknowledgements()
    }
}

impl GenerateConfig for KinesisFirehoseSinkConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"stream_name = "my-stream"
            encoding.codec = "json""#,
        )
        .unwrap()
    }
}

#[derive(Clone, Default)]
struct KinesisRetryLogic;

impl RetryLogic for KinesisRetryLogic {
    type Error = SdkError<KinesisError>;
    type Response = KinesisResponse;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        if let SdkError::ServiceError(inner) = error {
            if let PutRecordBatchErrorKind::ServiceUnavailableException(_) = inner.err().kind {
                return true;
            }
        }
        is_retriable_error(error)
    }
}
