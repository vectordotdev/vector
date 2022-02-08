use std::num::NonZeroU64;

use futures::FutureExt;
use rusoto_core::RusotoError;
use rusoto_firehose::{
    DescribeDeliveryStreamError, DescribeDeliveryStreamInput, KinesisFirehose,
    KinesisFirehoseClient, PutRecordBatchError,
};
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use tower::ServiceBuilder;

use crate::{
    aws::{rusoto, AwsAuthentication, RegionOrEndpoint},
    config::{DataType, GenerateConfig, ProxyConfig, SinkConfig, SinkContext},
    sinks::{
        aws_kinesis_firehose::{
            request_builder::KinesisRequestBuilder,
            service::{KinesisResponse, KinesisService},
            sink::KinesisSink,
        },
        util::{
            encoding::{EncodingConfig, StandardEncodings},
            retries::RetryLogic,
            BatchConfig, Compression, ServiceBuilderExt, SinkBatchSettings, TowerRequestConfig,
        },
        Healthcheck, VectorSink,
    },
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
    const TIMEOUT_SECS: NonZeroU64 = unsafe { NonZeroU64::new_unchecked(1) };
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct KinesisFirehoseSinkConfig {
    pub stream_name: String,
    #[serde(flatten)]
    pub region: RegionOrEndpoint,
    pub encoding: EncodingConfig<StandardEncodings>,
    #[serde(default)]
    pub compression: Compression,
    #[serde(default)]
    pub batch: BatchConfig<KinesisFirehoseDefaultBatchSettings>,
    #[serde(default)]
    pub request: TowerRequestConfig,
    // Deprecated name. Moved to auth.
    pub assume_role: Option<String>,
    #[serde(default)]
    pub auth: AwsAuthentication,
}

#[derive(Debug, PartialEq, Snafu)]
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

#[derive(Debug, Snafu)]
enum HealthcheckError {
    #[snafu(display("DescribeDeliveryStream failed: {}", source))]
    DescribeDeliveryStreamFailed {
        source: RusotoError<DescribeDeliveryStreamError>,
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

#[async_trait::async_trait]
#[typetag::serde(name = "aws_kinesis_firehose")]
impl SinkConfig for KinesisFirehoseSinkConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let client = self.create_client(&cx.proxy)?;
        let healthcheck = self.clone().healthcheck(client.clone()).boxed();

        let batch_settings = self
            .batch
            .validate()?
            .limit_max_bytes(MAX_PAYLOAD_SIZE)?
            .limit_max_events(MAX_PAYLOAD_EVENTS)?
            .into_batcher_settings()?;

        let request_limits = self.request.unwrap_with(&TowerRequestConfig::default());

        let region = self.region.clone().try_into()?;
        let service = ServiceBuilder::new()
            .settings(request_limits, KinesisRetryLogic)
            .service(KinesisService {
                client,
                region,
                stream_name: self.stream_name.clone(),
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
        };
        Ok((VectorSink::from_event_streamsink(sink), healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "aws_kinesis_firehose"
    }
}

impl KinesisFirehoseSinkConfig {
    async fn healthcheck(self, client: KinesisFirehoseClient) -> crate::Result<()> {
        let stream_name = self.stream_name;

        let req = client.describe_delivery_stream(DescribeDeliveryStreamInput {
            delivery_stream_name: stream_name.clone(),
            exclusive_start_destination_id: None,
            limit: Some(1),
        });

        match req.await {
            Ok(resp) => {
                let name = resp.delivery_stream_description.delivery_stream_name;
                if name == stream_name {
                    Ok(())
                } else {
                    Err(HealthcheckError::StreamNamesMismatch { name, stream_name }.into())
                }
            }
            Err(source) => Err(HealthcheckError::DescribeDeliveryStreamFailed { source }.into()),
        }
    }

    pub fn create_client(&self, proxy: &ProxyConfig) -> crate::Result<KinesisFirehoseClient> {
        let region = (&self.region).try_into()?;

        let client = rusoto::client(proxy)?;
        let creds = self.auth.build(&region, self.assume_role.clone())?;

        let client = rusoto_core::Client::new_with_encoding(creds, client, self.compression.into());
        Ok(KinesisFirehoseClient::new_with_client(client, region))
    }
}

#[derive(Clone)]
pub struct KinesisRetryLogic;

impl RetryLogic for KinesisRetryLogic {
    type Error = RusotoError<PutRecordBatchError>;
    type Response = KinesisResponse;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        match error {
            RusotoError::Service(PutRecordBatchError::ServiceUnavailable(_)) => true,
            error => rusoto::is_retriable_error(error),
        }
    }
}
