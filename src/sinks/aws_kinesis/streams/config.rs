use aws_sdk_kinesis::operation::{
    describe_stream::DescribeStreamError, put_records::PutRecordsError,
};
use aws_smithy_runtime_api::client::{orchestrator::HttpResponse, result::SdkError};
use futures::FutureExt;
use snafu::Snafu;
use vector_lib::configurable::{component::GenerateConfig, configurable_component};

use super::{
    KinesisClient, KinesisError, KinesisRecord, KinesisResponse, KinesisSinkBaseConfig, build_sink,
    record::{KinesisStreamClient, KinesisStreamRecord},
    sink::BatchKinesisRequest,
};
use crate::{
    aws::{ClientBuilder, create_client, is_retriable_error},
    config::{AcknowledgementsConfig, Input, ProxyConfig, SinkConfig, SinkContext},
    sinks::{
        Healthcheck, VectorSink,
        prelude::*,
        util::{
            BatchConfig, SinkBatchSettings,
            retries::{RetryAction, RetryLogic},
        },
    },
};

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Snafu)]
enum HealthcheckError {
    #[snafu(display("DescribeStream failed: {}", source))]
    DescribeStreamFailed {
        source: SdkError<DescribeStreamError, HttpResponse>,
    },
    #[snafu(display("Stream names do not match, got {}, expected {}", name, stream_name))]
    StreamNamesMismatch { name: String, stream_name: String },
}

pub struct KinesisClientBuilder;

impl ClientBuilder for KinesisClientBuilder {
    type Client = KinesisClient;

    fn build(&self, config: &aws_types::SdkConfig) -> Self::Client {
        KinesisClient::new(config)
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
#[configurable_component(sink(
    "aws_kinesis_streams",
    "Publish logs to AWS Kinesis Streams topics."
))]
#[derive(Clone, Debug)]
pub struct KinesisStreamsSinkConfig {
    #[serde(flatten)]
    pub base: KinesisSinkBaseConfig,

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
                    .map(|x| x.stream_name)
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
            &KinesisClientBuilder {},
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
#[typetag::serde(name = "aws_kinesis_streams")]
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
            self.base.partition_key_field.clone(),
            batch_settings,
            KinesisStreamClient { client },
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
struct KinesisRetryLogic {
    retry_partial: bool,
}

impl RetryLogic for KinesisRetryLogic {
    type Error = SdkError<KinesisError, HttpResponse>;
    type Request = BatchKinesisRequest<KinesisStreamRecord>;
    type Response = KinesisResponse;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        if let SdkError::ServiceError(inner) = error {
            // Note that if the request partially fails (records sent to one
            // partition fail but the others do not, for example), Vector
            // does not retry. This line only covers a failure for the entire
            // request.
            //
            // https://github.com/vectordotdev/vector/issues/359
            if matches!(
                inner.err(),
                PutRecordsError::ProvisionedThroughputExceededException(_)
            ) {
                return true;
            }
        }
        is_retriable_error(error)
    }

    fn should_retry_response(&self, response: &Self::Response) -> RetryAction<Self::Request> {
        if response.failure_count > 0 && self.retry_partial && !response.failed_records.is_empty() {
            let failed_records = response.failed_records.clone();
            RetryAction::RetryPartial(Box::new(move |original_request| {
                let failed_events: Vec<_> = failed_records
                    .iter()
                    .filter_map(|r| original_request.events.get(r.index).cloned())
                    .collect();

                let metadata = RequestMetadata::from_batch(
                    failed_events.iter().map(|req| req.get_metadata().clone()),
                );

                BatchKinesisRequest {
                    events: failed_events,
                    metadata,
                }
            }))
        } else {
            RetryAction::Successful
        }
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
