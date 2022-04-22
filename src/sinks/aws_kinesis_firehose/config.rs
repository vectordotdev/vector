use aws_sdk_firehose::error::{
    DescribeDeliveryStreamError, PutRecordBatchError, PutRecordBatchErrorKind,
};
use aws_sdk_firehose::types::SdkError;

use aws_sdk_firehose::{Client as KinesisFirehoseClient, Endpoint, Region};
use aws_smithy_client::erase::DynConnector;
use aws_types::credentials::SharedCredentialsProvider;
use futures::FutureExt;
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use tower::ServiceBuilder;

use crate::{
    aws::{create_client, is_retriable_error, AwsAuthentication, ClientBuilder, RegionOrEndpoint},
    codecs::Encoder,
    config::{AcknowledgementsConfig, GenerateConfig, Input, ProxyConfig, SinkConfig, SinkContext},
    sinks::{
        aws_kinesis_firehose::{
            request_builder::KinesisRequestBuilder,
            service::{KinesisResponse, KinesisService},
            sink::KinesisSink,
        },
        util::{
            encoding::{
                EncodingConfig, EncodingConfigAdapter, StandardEncodings, StandardEncodingsMigrator,
            },
            retries::RetryLogic,
            BatchConfig, Compression, ServiceBuilderExt, SinkBatchSettings, TowerRequestConfig,
        },
        Healthcheck, VectorSink,
    },
    tls::TlsOptions,
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

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct KinesisFirehoseSinkConfig {
    pub stream_name: String,
    #[serde(flatten)]
    pub region: RegionOrEndpoint,
    #[serde(flatten)]
    pub encoding:
        EncodingConfigAdapter<EncodingConfig<StandardEncodings>, StandardEncodingsMigrator>,
    #[serde(default)]
    pub compression: Compression,
    #[serde(default)]
    pub batch: BatchConfig<KinesisFirehoseDefaultBatchSettings>,
    #[serde(default)]
    pub request: TowerRequestConfig,
    pub tls: Option<TlsOptions>,
    #[serde(default)]
    pub auth: AwsAuthentication,
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
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
    type ConfigBuilder = aws_sdk_firehose::config::Builder;
    type Client = aws_sdk_firehose::Client;

    fn create_config_builder(
        credentials_provider: SharedCredentialsProvider,
    ) -> Self::ConfigBuilder {
        Self::ConfigBuilder::new().credentials_provider(credentials_provider)
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
        let serializer = self.encoding.clone().encoding();
        let encoder = Encoder::<()>::new(serializer);

        let request_builder = KinesisRequestBuilder {
            compression: self.compression,
            encoder: (transformer, encoder),
        };

        let sink = KinesisSink {
            batch_settings,
            acker: cx.acker(),
            service,
            request_builder,
        };
        Ok((VectorSink::from_event_streamsink(sink), healthcheck))
    }

    fn input(&self) -> Input {
        Input::log()
    }

    fn sink_type(&self) -> &'static str {
        "aws_kinesis_firehose"
    }

    fn acknowledgements(&self) -> Option<&AcknowledgementsConfig> {
        Some(&self.acknowledgements)
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
