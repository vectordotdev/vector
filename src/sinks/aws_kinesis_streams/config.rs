use std::{convert::TryInto, num::NonZeroU64};

use futures::FutureExt;
use rusoto_core::RusotoError;
use rusoto_kinesis::{DescribeStreamInput, Kinesis, KinesisClient, PutRecordsError};
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use tower::ServiceBuilder;

use super::service::KinesisResponse;
use crate::{
    aws::{
        rusoto,
        rusoto::{AwsAuthentication, RegionOrEndpoint},
    },
    config::{DataType, GenerateConfig, ProxyConfig, SinkConfig, SinkContext},
    sinks::{
        aws_kinesis_streams::{
            request_builder::KinesisRequestBuilder, service::KinesisService, sink::KinesisSink,
        },
        util::{
            encoding::{EncodingConfig, StandardEncodings},
            retries::RetryLogic,
            BatchConfig, Compression, ServiceBuilderExt, SinkBatchSettings, TowerRequestConfig,
        },
        Healthcheck, VectorSink,
    },
};

#[derive(Debug, Snafu)]
enum HealthcheckError {
    #[snafu(display("DescribeStream failed: {}", source))]
    DescribeStreamFailed {
        source: RusotoError<rusoto_kinesis::DescribeStreamError>,
    },
    #[snafu(display("Stream names do not match, got {}, expected {}", name, stream_name))]
    StreamNamesMismatch { name: String, stream_name: String },
    #[snafu(display(
        "Stream returned does not contain any streams that match {}",
        stream_name
    ))]
    NoMatchingStreamName { stream_name: String },
}

#[derive(Clone, Copy, Debug, Default)]
pub struct KinesisDefaultBatchSettings;

impl SinkBatchSettings for KinesisDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = Some(500);
    const MAX_BYTES: Option<usize> = Some(5_000_000);
    const TIMEOUT_SECS: NonZeroU64 = unsafe { NonZeroU64::new_unchecked(1) };
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct KinesisSinkConfig {
    pub stream_name: String,
    pub partition_key_field: Option<String>,
    #[serde(flatten)]
    pub region: RegionOrEndpoint,
    pub encoding: EncodingConfig<StandardEncodings>,
    #[serde(default)]
    pub compression: Compression,
    #[serde(default)]
    pub batch: BatchConfig<KinesisDefaultBatchSettings>,
    #[serde(default)]
    pub request: TowerRequestConfig,
    // Deprecated name. Moved to auth.
    pub assume_role: Option<String>,
    #[serde(default)]
    pub auth: AwsAuthentication,
}

impl KinesisSinkConfig {
    async fn healthcheck(self, client: KinesisClient) -> crate::Result<()> {
        let stream_name = self.stream_name;

        let req = client.describe_stream(DescribeStreamInput {
            stream_name: stream_name.clone(),
            exclusive_start_shard_id: None,
            limit: Some(1),
        });

        match req.await {
            Ok(resp) => {
                let name = resp.stream_description.stream_name;
                if name == stream_name {
                    Ok(())
                } else {
                    Err(HealthcheckError::StreamNamesMismatch { name, stream_name }.into())
                }
            }
            Err(source) => Err(HealthcheckError::DescribeStreamFailed { source }.into()),
        }
    }

    pub fn create_client(&self, proxy: &ProxyConfig) -> crate::Result<KinesisClient> {
        let region = (&self.region).try_into()?;

        let client = rusoto::client(proxy)?;
        let creds = self.auth.build(&region, self.assume_role.clone())?;

        let client = rusoto_core::Client::new_with_encoding(creds, client, self.compression.into());
        Ok(KinesisClient::new_with_client(client, region))
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "aws_kinesis_streams")]
impl SinkConfig for KinesisSinkConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let client = self.create_client(&cx.proxy)?;
        let healthcheck = self.clone().healthcheck(client.clone()).boxed();

        let batch_settings = self.batch.into_batcher_settings()?;

        let request_limits = self.request.unwrap_with(&TowerRequestConfig::default());

        let region = self.region.clone().try_into()?;
        let service = ServiceBuilder::new()
            .settings(request_limits, KinesisRetryLogic)
            .service(KinesisService {
                client,
                stream_name: self.stream_name.clone(),
                region,
            });

        let request_builder = KinesisRequestBuilder {
            compression: self.compression,
            encoder: self.encoding.clone(),
        };

        let sink = KinesisSink {
            batch_settings,
            acker: cx.acker(),
            service,
            request_builder,
            partition_key_field: self.partition_key_field.clone(),
        };
        Ok((VectorSink::from_event_streamsink(sink), healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "aws_kinesis_streams"
    }
}

impl GenerateConfig for KinesisSinkConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"region = "us-east-1"
            stream_name = "my-stream"
            encoding.codec = "json""#,
        )
        .unwrap()
    }
}

#[derive(Debug, Clone)]
struct KinesisRetryLogic;

impl RetryLogic for KinesisRetryLogic {
    type Error = RusotoError<PutRecordsError>;
    type Response = KinesisResponse;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        match error {
            RusotoError::Service(PutRecordsError::ProvisionedThroughputExceeded(_)) => true,
            error => rusoto::is_retriable_error(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::sinks::aws_kinesis_streams::config::KinesisSinkConfig;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<KinesisSinkConfig>();
    }
}
