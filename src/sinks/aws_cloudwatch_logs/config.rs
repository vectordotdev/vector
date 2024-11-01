use aws_sdk_cloudwatchlogs::Client as CloudwatchLogsClient;
use futures::FutureExt;
use serde::{de, Deserialize, Deserializer};
use tower::ServiceBuilder;
use vector_lib::codecs::JsonSerializerConfig;
use vector_lib::configurable::configurable_component;
use vector_lib::schema;
use vrl::value::Kind;

use crate::{
    aws::{create_client, AwsAuthentication, ClientBuilder, RegionOrEndpoint},
    codecs::{Encoder, EncodingConfig},
    config::{
        AcknowledgementsConfig, DataType, GenerateConfig, Input, ProxyConfig, SinkConfig,
        SinkContext,
    },
    sinks::{
        aws_cloudwatch_logs::{
            healthcheck::healthcheck, request_builder::CloudwatchRequestBuilder,
            retry::CloudwatchRetryLogic, service::CloudwatchLogsPartitionSvc, sink::CloudwatchSink,
        },
        util::{
            http::RequestConfig, BatchConfig, Compression, ServiceBuilderExt, SinkBatchSettings,
        },
        Healthcheck, VectorSink,
    },
    template::Template,
    tls::TlsConfig,
};

pub struct CloudwatchLogsClientBuilder;

impl ClientBuilder for CloudwatchLogsClientBuilder {
    type Client = aws_sdk_cloudwatchlogs::client::Client;

    fn build(config: &aws_types::SdkConfig) -> Self::Client {
        aws_sdk_cloudwatchlogs::client::Client::new(config)
    }
}

#[configurable_component]
#[derive(Clone, Debug, Default)]
/// Retention policy configuration for AWS CloudWatch Log Group
pub struct Retention {
    /// Whether or not to set a retention policy when creating a new Log Group.
    #[serde(default)]
    pub enabled: bool,

    /// If retention is enabled, the number of days to retain logs for.
    #[serde(
        default,
        deserialize_with = "retention_days",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub days: u32,
}

fn retention_days<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    let days: u32 = Deserialize::deserialize(deserializer)?;
    const ALLOWED_VALUES: &[u32] = &[
        1, 3, 5, 7, 14, 30, 60, 90, 120, 150, 180, 365, 400, 545, 731, 1096, 1827, 2192, 2557,
        2922, 3288, 3653,
    ];
    if ALLOWED_VALUES.contains(&days) {
        Ok(days)
    } else {
        let msg = format!("one of allowed values: {:?}", ALLOWED_VALUES).to_owned();
        let expected: &str = &msg[..];
        Err(de::Error::invalid_value(
            de::Unexpected::Signed(days.into()),
            &expected,
        ))
    }
}

/// Configuration for the `aws_cloudwatch_logs` sink.
#[configurable_component(sink(
    "aws_cloudwatch_logs",
    "Publish log events to AWS CloudWatch Logs."
))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct CloudwatchLogsSinkConfig {
    /// The [group name][group_name] of the target CloudWatch Logs stream.
    ///
    /// [group_name]: https://docs.aws.amazon.com/AmazonCloudWatch/latest/logs/Working-with-log-groups-and-streams.html
    #[configurable(metadata(docs::examples = "group-name"))]
    #[configurable(metadata(docs::examples = "{{ file }}"))]
    pub group_name: Template,

    /// The [stream name][stream_name] of the target CloudWatch Logs stream.
    ///
    /// There can only be one writer to a log stream at a time. If multiple instances are writing to
    /// the same log group, the stream name must include an identifier that is guaranteed to be
    /// unique per instance.
    ///
    /// [stream_name]: https://docs.aws.amazon.com/AmazonCloudWatch/latest/logs/Working-with-log-groups-and-streams.html
    #[configurable(metadata(docs::examples = "{{ host }}"))]
    #[configurable(metadata(docs::examples = "%Y-%m-%d"))]
    #[configurable(metadata(docs::examples = "stream-name"))]
    pub stream_name: Template,

    /// The [AWS region][aws_region] of the target service.
    ///
    /// [aws_region]: https://docs.aws.amazon.com/AmazonRDS/latest/UserGuide/Concepts.RegionsAndAvailabilityZones.html
    #[serde(flatten)]
    pub region: RegionOrEndpoint,

    /// Dynamically create a [log group][log_group] if it does not already exist.
    ///
    /// This ignores `create_missing_stream` directly after creating the group and creates
    /// the first stream.
    ///
    /// [log_group]: https://docs.aws.amazon.com/AmazonCloudWatch/latest/logs/Working-with-log-groups-and-streams.html
    #[serde(default = "crate::serde::default_true")]
    pub create_missing_group: bool,

    /// Dynamically create a [log stream][log_stream] if it does not already exist.
    ///
    /// [log_stream]: https://docs.aws.amazon.com/AmazonCloudWatch/latest/logs/Working-with-log-groups-and-streams.html
    #[serde(default = "crate::serde::default_true")]
    pub create_missing_stream: bool,

    #[configurable(derived)]
    #[serde(default)]
    pub retention: Retention,

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
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
}

impl CloudwatchLogsSinkConfig {
    pub async fn create_client(&self, proxy: &ProxyConfig) -> crate::Result<CloudwatchLogsClient> {
        create_client::<CloudwatchLogsClientBuilder>(
            &self.auth,
            self.region.region(),
            self.region.endpoint(),
            proxy,
            &self.tls,
            &None,
        )
        .await
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "aws_cloudwatch_logs")]
impl SinkConfig for CloudwatchLogsSinkConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let batcher_settings = self.batch.into_batcher_settings()?;
        let request_settings = self.request.tower.into_settings();
        let client = self.create_client(cx.proxy()).await?;
        let svc = ServiceBuilder::new()
            .settings(request_settings, CloudwatchRetryLogic::new())
            .service(CloudwatchLogsPartitionSvc::new(
                self.clone(),
                client.clone(),
            )?);
        let transformer = self.encoding.transformer();
        let serializer = self.encoding.build()?;
        let encoder = Encoder::<()>::new(serializer);
        let healthcheck = healthcheck(self.clone(), client).boxed();
        let sink = CloudwatchSink {
            batcher_settings,
            request_builder: CloudwatchRequestBuilder {
                group_template: self.group_name.clone(),
                stream_template: self.stream_name.clone(),
                transformer,
                encoder,
            },

            service: svc,
        };

        Ok((VectorSink::from_event_streamsink(sink), healthcheck))
    }

    fn input(&self) -> Input {
        let requirement =
            schema::Requirement::empty().optional_meaning("timestamp", Kind::timestamp());

        Input::new(self.encoding.config().input_type() & DataType::Log)
            .with_schema_requirement(requirement)
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
        create_missing_group: true,
        create_missing_stream: true,
        retention: Default::default(),
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
