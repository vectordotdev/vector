use aws_sdk_kinesis::{
    error::{DescribeStreamError, PutRecordsErrorKind},
    types::SdkError,
};
use futures::FutureExt;
use snafu::Snafu;
use vector_config::{component::GenerateConfig, configurable_component};

use crate::{
    aws::{create_client, is_retriable_error, ClientBuilder},
    config::{AcknowledgementsConfig, Input, ProxyConfig, SinkConfig, SinkContext},
    sinks::{
        util::{retries::RetryLogic, BatchConfig, SinkBatchSettings},
        Healthcheck, VectorSink,
    },
};

use super::{
    build_sink,
    record::{KinesisStreamClient, KinesisStreamRecord},
    KinesisClient, KinesisError, KinesisRecord, KinesisResponse, KinesisSinkBaseConfig,
};

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Snafu)]
enum HealthcheckError {
    #[snafu(display("DescribeStream failed: {}", source))]
    DescribeStreamFailed {
        source: SdkError<DescribeStreamError>,
    },
    #[snafu(display("Stream names do not match, got {}, expected {}", name, stream_name))]
    StreamNamesMismatch { name: String, stream_name: String },
    #[snafu(display(
        "Stream returned does not contain any streams that match {}",
        stream_name
    ))]
    NoMatchingStreamName { stream_name: String },
}

pub struct KinesisClientBuilder;

impl ClientBuilder for KinesisClientBuilder {
    type Config = aws_sdk_kinesis::config::Config;
    type Client = KinesisClient;
    type DefaultMiddleware = aws_sdk_kinesis::middleware::DefaultMiddleware;

    fn default_middleware() -> Self::DefaultMiddleware {
        aws_sdk_kinesis::middleware::DefaultMiddleware::new()
    }

    fn build(client: aws_smithy_client::Client, config: &aws_types::SdkConfig) -> Self::Client {
        KinesisClient::with_config(client, config.into())
    }
}

pub const MAX_PAYLOAD_SIZE: usize = 5_000_000;
pub const MAX_PAYLOAD_EVENTS: usize = 500;

#[derive(Clone, Copy, Debug, Default)]
pub struct KinesisDefaultBatchSettings;

impl SinkBatchSettings for KinesisDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = Some(MAX_PAYLOAD_EVENTS);
    const MAX_BYTES: Option<usize> = Some(MAX_PAYLOAD_SIZE);
    const TIMEOUT_SECS: f64 = 1.0;
}

/// Configuration for the `aws_kinesis_streams` sink.
#[configurable_component(sink("aws_kinesis_streams"))]
#[derive(Clone, Debug)]
pub struct KinesisStreamsSinkConfig {
    #[serde(flatten)]
    pub base: KinesisSinkBaseConfig,

    /// The log field used as the Kinesis record’s partition key value.
    ///
    /// If not specified, a unique partition key will be generated for each Kinesis record.
    #[configurable(metadata(docs::examples = "user_id"))]
    pub partition_key_field: Option<String>,

    #[configurable(derived)]
    #[serde(default)]
    pub batch: BatchConfig<KinesisDefaultBatchSettings>,
}

impl KinesisStreamsSinkConfig {
    async fn healthcheck(self, client: KinesisClient) -> crate::Result<()> {
        let stream_name = self.base.stream_name;

        let describe_result = client
            .describe_stream()
            .stream_name(stream_name.clone())
            .set_exclusive_start_shard_id(None)
            .limit(1)
            .send()
            .await;

        match describe_result {
            Ok(resp) => {
                let name = resp
                    .stream_description
                    .and_then(|x| x.stream_name)
                    .unwrap_or_default();
                if name == stream_name {
                    Ok(())
                } else {
                    Err(HealthcheckError::StreamNamesMismatch { name, stream_name }.into())
                }
            }
            Err(source) => Err(HealthcheckError::DescribeStreamFailed { source }.into()),
        }
    }

    pub async fn create_client(&self, proxy: &ProxyConfig) -> crate::Result<KinesisClient> {
        create_client::<KinesisClientBuilder>(
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
impl SinkConfig for KinesisStreamsSinkConfig {
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
            KinesisStreamClient,
            KinesisRecord,
            KinesisStreamRecord,
            KinesisError,
            KinesisRetryLogic,
        >(
            &self.base,
            self.partition_key_field.clone(),
            batch_settings,
            KinesisStreamClient { client },
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

impl GenerateConfig for KinesisStreamsSinkConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"partition_key_field = "foo"
            stream_name = "my-stream"
            encoding.codec = "json""#,
        )
        .unwrap()
    }
}
#[derive(Default, Clone)]
struct KinesisRetryLogic;

impl RetryLogic for KinesisRetryLogic {
    type Error = SdkError<KinesisError>;
    type Response = KinesisResponse;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        if let SdkError::ServiceError(inner) = error {
            if let PutRecordsErrorKind::ProvisionedThroughputExceededException(_) = inner.err().kind
            {
                return true;
            }
        }
        is_retriable_error(error)
    }
}

#[cfg(test)]
mod tests {
    use super::KinesisStreamsSinkConfig;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<KinesisStreamsSinkConfig>();
    }
}
