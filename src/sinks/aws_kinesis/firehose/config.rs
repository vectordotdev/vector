use aws_sdk_firehose::operation::{
    describe_delivery_stream::DescribeDeliveryStreamError, put_record_batch::PutRecordBatchError,
};
use aws_smithy_runtime_api::client::{orchestrator::HttpResponse, result::SdkError};
use futures::FutureExt;
use snafu::Snafu;
use vector_lib::configurable::configurable_component;
use vector_lib::stream::BatcherSettings;

use super::{
    KinesisClient, KinesisError, KinesisRecord, KinesisResponse, KinesisSinkBaseConfig, build_sink,
    record::{KinesisFirehoseClient, KinesisFirehoseRecord},
    sink::BatchKinesisRequest,
};
use crate::{
    aws::{ClientBuilder, create_client, is_retriable_error},
    config::{
        AcknowledgementsConfig, GenerateConfig, Input, ProxyConfig, SinkConfig, SinkContext,
        ValidatedSink,
    },
    sinks::{
        Healthcheck, VectorSink,
        util::{
            BatchConfig, SinkBatchSettings,
            retries::{RetryAction, RetryLogic},
        },
    },
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

    fn build(&self, config: &aws_types::SdkConfig) -> Self::Client {
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
            &KinesisFirehoseClientBuilder {},
            &self.base.auth,
            self.base.region.region(),
            self.base.region.endpoint(),
            proxy,
            self.base.tls.as_ref(),
            None,
        )
        .await
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "aws_kinesis_firehose")]
impl SinkConfig for KinesisFirehoseSinkConfig {
    fn input(&self) -> Input {
        self.base.input()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        self.base.acknowledgements()
    }
}

#[derive(Clone, Debug)]
pub struct ValidatedKinesisFirehose {
    batch_settings: BatcherSettings,
}

#[async_trait::async_trait]
impl ValidatedSink for KinesisFirehoseSinkConfig {
    type Validated = ValidatedKinesisFirehose;

    fn validate(&self) -> crate::Result<ValidatedKinesisFirehose> {
        let batch_settings = self
            .batch
            .validate()?
            .limit_max_bytes(MAX_PAYLOAD_SIZE)?
            .limit_max_events(MAX_PAYLOAD_EVENTS)?
            .into_batcher_settings()?;

        Ok(ValidatedKinesisFirehose { batch_settings })
    }

    async fn build(
        &self,
        validated: &ValidatedKinesisFirehose,
        cx: SinkContext,
    ) -> crate::Result<(VectorSink, Healthcheck)> {
        let client = self.create_client(&cx.proxy).await?;
        let healthcheck = self.clone().healthcheck(client.clone()).boxed();

        let sink = build_sink::<
            KinesisFirehoseClient,
            KinesisRecord,
            KinesisFirehoseRecord,
            KinesisError,
            KinesisRetryLogic,
        >(
            &self.base,
            self.base.partition_key_field.clone(),
            validated.batch_settings,
            KinesisFirehoseClient { client },
            KinesisRetryLogic {
                retry_partial: self.base.request_retry_partial,
            },
        )?;

        Ok((sink, healthcheck))
    }
}

impl GenerateConfig for KinesisFirehoseSinkConfig {
    fn generate_config() -> serde_json::Value {
        serde_yaml::from_str(indoc::indoc! {
            r#"stream_name: my-stream
            encoding:
              codec: json"#,
        })
        .unwrap()
    }
}

#[derive(Clone, Default)]
struct KinesisRetryLogic {
    retry_partial: bool,
}

impl RetryLogic for KinesisRetryLogic {
    type Error = SdkError<KinesisError, HttpResponse>;
    type Request = BatchKinesisRequest<KinesisFirehoseRecord>;
    type Response = KinesisResponse;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        if let SdkError::ServiceError(inner) = error
            && matches!(
                inner.err(),
                PutRecordBatchError::ServiceUnavailableException(_)
            )
        {
            return true;
        }
        is_retriable_error(error)
    }

    fn should_retry_response(&self, response: &Self::Response) -> RetryAction<Self::Request> {
        if response.failure_count > 0 && self.retry_partial {
            let msg = format!("partial error count {}", response.failure_count);
            RetryAction::Retry(msg.into())
        } else {
            RetryAction::Successful
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_produces_batch_settings() {
        let config = KinesisFirehoseSinkConfig {
            batch: BatchConfig::<KinesisFirehoseDefaultBatchSettings>::default(),
            base: KinesisSinkBaseConfig {
                stream_name: String::from("test"),
                region: crate::aws::RegionOrEndpoint::with_both(
                    "us-east-1",
                    "http://localhost:4566",
                ),
                encoding: vector_lib::codecs::JsonSerializerConfig::default().into(),
                compression: crate::sinks::util::Compression::None,
                request: Default::default(),
                tls: None,
                auth: Default::default(),
                request_retry_partial: false,
                acknowledgements: Default::default(),
                partition_key_field: None,
            },
        };

        let validated = config.validate().expect("validation should succeed");
        assert_eq!(validated.batch_settings.item_limit, MAX_PAYLOAD_EVENTS);
        assert_eq!(validated.batch_settings.size_limit, MAX_PAYLOAD_SIZE);
    }
}
