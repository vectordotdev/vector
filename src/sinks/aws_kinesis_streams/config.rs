use aws_sdk_kinesis::error::{DescribeStreamError, PutRecordsError, PutRecordsErrorKind};
use aws_sdk_kinesis::types::SdkError;

use aws_sdk_kinesis::{Client as KinesisClient, Endpoint, Region};
use aws_smithy_client::erase::DynConnector;
use aws_types::credentials::SharedCredentialsProvider;
use futures::FutureExt;
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use tower::ServiceBuilder;

use super::service::KinesisResponse;
use crate::{
    aws::{create_client, is_retriable_error, AwsAuthentication, ClientBuilder, RegionOrEndpoint},
    codecs::Encoder,
    config::{AcknowledgementsConfig, GenerateConfig, Input, ProxyConfig, SinkConfig, SinkContext},
    sinks::{
        aws_kinesis_streams::{
            request_builder::KinesisRequestBuilder, service::KinesisService, sink::KinesisSink,
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
    tls::TlsConfig,
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
    type ConfigBuilder = aws_sdk_kinesis::config::Builder;
    type Client = KinesisClient;

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

#[derive(Clone, Copy, Debug, Default)]
pub struct KinesisDefaultBatchSettings;

impl SinkBatchSettings for KinesisDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = Some(500);
    const MAX_BYTES: Option<usize> = Some(5_000_000);
    const TIMEOUT_SECS: f64 = 1.0;
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct KinesisSinkConfig {
    pub stream_name: String,
    pub partition_key_field: Option<String>,
    #[serde(flatten)]
    pub region: RegionOrEndpoint,
    #[serde(flatten)]
    pub encoding:
        EncodingConfigAdapter<EncodingConfig<StandardEncodings>, StandardEncodingsMigrator>,
    #[serde(default)]
    pub compression: Compression,
    #[serde(default)]
    pub batch: BatchConfig<KinesisDefaultBatchSettings>,
    #[serde(default)]
    pub request: TowerRequestConfig,
    pub tls: Option<TlsConfig>,
    #[serde(default)]
    pub auth: AwsAuthentication,
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
}

impl KinesisSinkConfig {
    async fn healthcheck(self, client: KinesisClient) -> crate::Result<()> {
        let stream_name = self.stream_name;

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
            &self.auth,
            self.region.region(),
            self.region.endpoint()?,
            proxy,
            &self.tls,
        )
        .await
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "aws_kinesis_streams")]
impl SinkConfig for KinesisSinkConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let client = self.create_client(&cx.proxy).await?;
        let healthcheck = self.clone().healthcheck(client.clone()).boxed();

        let batch_settings = self.batch.into_batcher_settings()?;

        let request_settings = self.request.unwrap_with(&TowerRequestConfig::default());

        let region = self.region.region();
        let service = ServiceBuilder::new()
            .settings(request_settings, KinesisRetryLogic)
            .service(KinesisService {
                client,
                stream_name: self.stream_name.clone(),
                region,
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
            partition_key_field: self.partition_key_field.clone(),
        };
        Ok((VectorSink::from_event_streamsink(sink), healthcheck))
    }

    fn input(&self) -> Input {
        Input::log()
    }

    fn sink_type(&self) -> &'static str {
        "aws_kinesis_streams"
    }

    fn acknowledgements(&self) -> Option<&AcknowledgementsConfig> {
        Some(&self.acknowledgements)
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
    type Error = SdkError<PutRecordsError>;
    type Response = KinesisResponse;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        if let SdkError::ServiceError { err, raw: _ } = error {
            if let PutRecordsErrorKind::ProvisionedThroughputExceededException(_) = err.kind {
                return true;
            }
        }
        is_retriable_error(error)
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
