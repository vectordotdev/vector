use aws_sdk_firehose::operation::{
    describe_delivery_stream::DescribeDeliveryStreamError, put_record_batch::PutRecordBatchError,
};
use aws_smithy_runtime_api::client::{orchestrator::HttpResponse, result::SdkError};
use futures::FutureExt;
use snafu::Snafu;
use vector_lib::configurable::configurable_component;

use crate::sinks::util::retries::RetryAction;
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
        source: SdkError<DescribeDeliveryStreamError, HttpResponse>,
    },
    #[snafu(display("Stream name does not match, got {}, expected {}", name, stream_name))]
    StreamNamesMismatch { name: String, stream_name: String },
}

pub struct KinesisFirehoseClientBuilder;

impl ClientBuilder for KinesisFirehoseClientBuilder {
    type Client = KinesisClient;

    fn build(config: &aws_types::SdkConfig) -> Self::Client {
        Self::Client::new(config)
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
#[configurable_component(sink(
    "aws_kinesis_firehose",
    "Publish logs to AWS Kinesis Data Firehose topics."
))]
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
                    .map(|x| x.delivery_stream_name)
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
            self.base.region.endpoint(),
            proxy,
            &self.base.tls,
        )
        .await
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "aws_kinesis_firehose")]
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
            self.base.partition_key_field.clone(),
            batch_settings,
            KinesisFirehoseClient { client },
            KinesisRetryLogic {
                retry_partial: self.base.request_retry_partial,
            },
        )?;

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
struct KinesisRetryLogic {
    retry_partial: bool,
}

impl RetryLogic for KinesisRetryLogic {
    type Error = SdkError<KinesisError, HttpResponse>;
    type Response = KinesisResponse;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        if let SdkError::ServiceError(inner) = error {
            if matches!(
                inner.err(),
                PutRecordBatchError::ServiceUnavailableException(_)
            ) {
                return true;
            }
        }
        is_retriable_error(error)
    }

    fn should_retry_response(&self, response: &Self::Response) -> RetryAction {
        if response.failure_count > 0 && self.retry_partial {
            let msg = format!("partial error count {}", response.failure_count);
            RetryAction::Retry(msg.into())
        } else {
            RetryAction::Successful
        }
    }
}
