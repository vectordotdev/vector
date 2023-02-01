use aws_sdk_cloudwatchlogs::Client as CloudwatchLogsClient;
use aws_smithy_types::retry::RetryConfig;
use codecs::JsonSerializerConfig;
use futures::FutureExt;
use tower::ServiceBuilder;
use vector_config::configurable_component;

use crate::{
    aws::{
        create_client, create_smithy_client, resolve_region, AwsAuthentication, ClientBuilder,
        RegionOrEndpoint,
    },
    codecs::{Encoder, EncodingConfig},
    config::{
        log_schema, AcknowledgementsConfig, DataType, GenerateConfig, Input, ProxyConfig,
        SinkConfig, SinkContext,
    },
    sinks::{
        aws_cloudwatch_logs::{
            healthcheck::healthcheck, request_builder::CloudwatchRequestBuilder,
            retry::CloudwatchRetryLogic, service::CloudwatchLogsPartitionSvc, sink::CloudwatchSink,
        },
        util::{
            http::RequestConfig, BatchConfig, Compression, ServiceBuilderExt, SinkBatchSettings,
            TowerRequestConfig,
        },
        Healthcheck, VectorSink,
    },
    template::Template,
    tls::TlsConfig,
};

pub struct CloudwatchLogsClientBuilder;

impl ClientBuilder for CloudwatchLogsClientBuilder {
    type Config = aws_sdk_cloudwatchlogs::config::Config;
    type Client = aws_sdk_cloudwatchlogs::client::Client;
    type DefaultMiddleware = aws_sdk_cloudwatchlogs::middleware::DefaultMiddleware;

    fn default_middleware() -> Self::DefaultMiddleware {
        aws_sdk_cloudwatchlogs::middleware::DefaultMiddleware::new()
    }

    fn build(client: aws_smithy_client::Client, config: &aws_types::SdkConfig) -> Self::Client {
        aws_sdk_cloudwatchlogs::client::Client::with_config(client, config.into())
    }
}

/// Configuration for the `aws_cloudwatch_logs` sink.
#[configurable_component(sink("aws_cloudwatch_logs"))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct CloudwatchLogsSinkConfig {
    /// The [group name][group_name] of the target CloudWatch Logs stream.
    ///
    /// [group_name]: https://docs.aws.amazon.com/AmazonCloudWatch/latest/logs/Working-with-log-groups-and-streams.html
    pub group_name: Template,

    /// The [stream name][stream_name] of the target CloudWatch Logs stream.
    ///
    /// There can only be one writer to a log stream at a time. If you have multiple
    /// instances writing to the same log group, you must include an identifier in the
    /// stream name that is guaranteed to be unique per instance.
    ///
    /// For example, you might choose `host`.
    ///
    /// [stream_name]: https://docs.aws.amazon.com/AmazonCloudWatch/latest/logs/Working-with-log-groups-and-streams.html
    pub stream_name: Template,

    /// The [AWS region][aws_region] of the target service.
    ///
    /// [aws_region]: https://docs.aws.amazon.com/AmazonRDS/latest/UserGuide/Concepts.RegionsAndAvailabilityZones.html
    #[serde(flatten)]
    pub region: RegionOrEndpoint,

    /// Dynamically create a [log group][log_group] if it does not already exist.
    ///
    /// This will ignore `create_missing_stream` directly after creating the group and will create
    /// the first stream.
    ///
    /// [log_group]: https://docs.aws.amazon.com/AmazonCloudWatch/latest/logs/Working-with-log-groups-and-streams.html
    pub create_missing_group: Option<bool>,

    /// Dynamically create a [log stream][log_stream] if it does not already exist.
    ///
    /// [log_stream]: https://docs.aws.amazon.com/AmazonCloudWatch/latest/logs/Working-with-log-groups-and-streams.html
    pub create_missing_stream: Option<bool>,

    #[configurable(derived)]
    pub encoding: EncodingConfig,

    #[configurable(derived)]
    #[serde(default)]
    pub compression: Compression,

    #[configurable(derived)]
    #[serde(default)]
    pub batch: BatchConfig<CloudwatchLogsDefaultBatchSettings>,

    #[configurable(derived)]
    #[serde(default)]
    pub request: RequestConfig,

    #[configurable(derived)]
    pub tls: Option<TlsConfig>,

    /// The ARN of an [IAM role][iam_role] to assume at startup.
    ///
    /// [iam_role]: https://docs.aws.amazon.com/IAM/latest/UserGuide/id_roles.html
    #[configurable(deprecated)]
    #[configurable(metadata(docs::hidden))]
    pub assume_role: Option<String>,

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

impl CloudwatchLogsSinkConfig {
    pub async fn create_client(&self, proxy: &ProxyConfig) -> crate::Result<CloudwatchLogsClient> {
        create_client::<CloudwatchLogsClientBuilder>(
            &self.auth,
            self.region.region(),
            self.region.endpoint()?,
            proxy,
            &self.tls,
            true,
        )
        .await
    }

    pub async fn create_smithy_client(
        &self,
        proxy: &ProxyConfig,
    ) -> crate::Result<aws_smithy_client::Client> {
        let region = resolve_region(self.region.region()).await?;
        create_smithy_client::<CloudwatchLogsClientBuilder>(
            region,
            proxy,
            &self.tls,
            true,
            RetryConfig::disabled(),
        )
        .await
    }
}

#[async_trait::async_trait]
impl SinkConfig for CloudwatchLogsSinkConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let batcher_settings = self.batch.into_batcher_settings()?;
        let request_settings = self
            .request
            .tower
            .unwrap_with(&TowerRequestConfig::default());
        let client = self.create_client(cx.proxy()).await?;
        let smithy_client = self.create_smithy_client(cx.proxy()).await?;
        let svc = ServiceBuilder::new()
            .settings(request_settings, CloudwatchRetryLogic::new())
            .service(CloudwatchLogsPartitionSvc::new(
                self.clone(),
                client.clone(),
                std::sync::Arc::new(smithy_client),
            ));
        let transformer = self.encoding.transformer();
        let serializer = self.encoding.build()?;
        let encoder = Encoder::<()>::new(serializer);
        let healthcheck = healthcheck(self.clone(), client).boxed();
        let sink = CloudwatchSink {
            batcher_settings,
            request_builder: CloudwatchRequestBuilder {
                group_template: self.group_name.clone(),
                stream_template: self.stream_name.clone(),
                log_schema: log_schema().clone(),
                transformer,
                encoder,
            },

            service: svc,
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

impl GenerateConfig for CloudwatchLogsSinkConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(default_config(JsonSerializerConfig::default().into())).unwrap()
    }
}

fn default_config(encoding: EncodingConfig) -> CloudwatchLogsSinkConfig {
    CloudwatchLogsSinkConfig {
        encoding,
        group_name: Default::default(),
        stream_name: Default::default(),
        region: Default::default(),
        create_missing_group: Default::default(),
        create_missing_stream: Default::default(),
        compression: Default::default(),
        batch: Default::default(),
        request: Default::default(),
        tls: Default::default(),
        assume_role: Default::default(),
        auth: Default::default(),
        acknowledgements: Default::default(),
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CloudwatchLogsDefaultBatchSettings;

impl SinkBatchSettings for CloudwatchLogsDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = Some(10_000);
    const MAX_BYTES: Option<usize> = Some(1_048_576);
    const TIMEOUT_SECS: f64 = 1.0;
}

#[cfg(test)]
mod tests {
    use crate::sinks::aws_cloudwatch_logs::config::CloudwatchLogsSinkConfig;

    #[test]
    fn test_generate_config() {
        crate::test_util::test_generate_config::<CloudwatchLogsSinkConfig>();
    }
}
