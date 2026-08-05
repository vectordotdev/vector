use std::collections::HashMap;

use aws_sdk_cloudwatchlogs::Client as CloudwatchLogsClient;
use futures::FutureExt;
use serde::{Deserialize, Deserializer, de};
use tower::ServiceBuilder;
use vector_lib::{
    codecs::JsonSerializerConfig, configurable::configurable_component, schema,
    stream::BatcherSettings,
};
use vrl::value::Kind;

use crate::{
    aws::{AwsAuthentication, ClientBuilder, RegionOrEndpoint, create_client},
    codecs::{Encoder, EncodingConfig},
    config::{
        AcknowledgementsConfig, DataType, DynValidatedSink, GenerateConfig, Input, ProxyConfig,
        SinkConfig, SinkContext, ValidatedSink,
    },
    sinks::{
        Healthcheck, VectorSink,
        aws_cloudwatch_logs::{
            healthcheck::healthcheck, request_builder::CloudwatchRequestBuilder,
            retry::CloudwatchRetryLogic, service::CloudwatchLogsPartitionSvc, sink::CloudwatchSink,
        },
        util::{
            BatchConfig, Compression, ServiceBuilderExt, SinkBatchSettings, http::RequestConfig,
        },
    },
    template::{ConfinedTemplate, ConfinementConfig, Template},
    tls::TlsConfig,
};

pub struct CloudwatchLogsClientBuilder;

impl ClientBuilder for CloudwatchLogsClientBuilder {
    type Client = aws_sdk_cloudwatchlogs::client::Client;

    fn build(&self, config: &aws_types::SdkConfig) -> Self::Client {
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
        let msg = format!("one of allowed values: {ALLOWED_VALUES:?}").to_owned();
        let expected: &str = msg.as_str();
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

    /// The [ARN][arn] (Amazon Resource Name) of the [KMS key][kms_key] to use when encrypting log data.
    ///
    /// [arn]: https://docs.aws.amazon.com/IAM/latest/UserGuide/reference-arns.html
    /// [kms_key]: https://docs.aws.amazon.com/kms/latest/developerguide/overview.html
    #[configurable(derived)]
    #[serde(default)]
    pub kms_key: Option<String>,

    /// The Key-value pairs to be applied as [tags][tags] to the log group and stream.
    ///
    /// [tags]: https://docs.aws.amazon.com/whitepapers/latest/tagging-best-practices/what-are-tags.html
    #[configurable(derived)]
    #[serde(default)]
    #[configurable(metadata(
        docs::additional_props_description = "A tag represented as a key-value pair"
    ))]
    pub tags: Option<HashMap<String, String>>,

    #[configurable(derived)]
    #[serde(flatten)]
    pub confinement: ConfinementConfig,
}

impl CloudwatchLogsSinkConfig {
    pub async fn create_client(&self, proxy: &ProxyConfig) -> crate::Result<CloudwatchLogsClient> {
        create_client::<CloudwatchLogsClientBuilder>(
            &CloudwatchLogsClientBuilder {},
            &self.auth,
            self.region.region(),
            self.region.endpoint(),
            proxy,
            self.tls.as_ref(),
            None,
        )
        .await
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "aws_cloudwatch_logs")]
impl SinkConfig for CloudwatchLogsSinkConfig {
    fn as_dyn_validated(&self) -> Option<&dyn DynValidatedSink> {
        Some(self)
    }

    fn confinement_config(&self) -> Option<&crate::template::ConfinementConfig> {
        Some(&self.confinement)
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

/// Purely validated `aws_cloudwatch_logs` sink configuration.
///
/// This type captures all validation results that can be computed purely from
/// configuration without network/filesystem/credentials/async operations.
/// The actual sink building consumes these values without recomputing them.
#[derive(Clone, Debug)]
pub struct ValidatedCloudwatchLogs {
    /// Confined group name template.
    group_template: ConfinedTemplate,
    /// Confined stream name template.
    stream_template: ConfinedTemplate,
    /// Batch settings computed during validation.
    batcher_settings: BatcherSettings,
}

#[async_trait::async_trait]
impl ValidatedSink for CloudwatchLogsSinkConfig {
    type Validated = ValidatedCloudwatchLogs;

    fn validate(&self) -> crate::Result<ValidatedCloudwatchLogs> {
        let group_template =
            self.group_name
                .clone()
                .confine(&self.confinement, Self::NAME, "group_name")?;
        let stream_template =
            self.stream_name
                .clone()
                .confine(&self.confinement, Self::NAME, "stream_name")?;
        let batcher_settings = self.batch.into_batcher_settings()?;

        Ok(ValidatedCloudwatchLogs {
            group_template,
            stream_template,
            batcher_settings,
        })
    }

    async fn build(
        &self,
        validated: &ValidatedCloudwatchLogs,
        cx: SinkContext,
    ) -> crate::Result<(VectorSink, Healthcheck)> {
        let ValidatedCloudwatchLogs {
            group_template,
            stream_template,
            batcher_settings,
        } = validated;
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
            batcher_settings: *batcher_settings,
            request_builder: CloudwatchRequestBuilder {
                group_template: group_template.clone(),
                stream_template: stream_template.clone(),
                transformer,
                encoder,
            },

            service: svc,
        };
        Ok((VectorSink::from_event_streamsink(sink), healthcheck))
    }
}

impl GenerateConfig for CloudwatchLogsSinkConfig {
    fn generate_config() -> serde_json::Value {
        serde_json::to_value(default_config(JsonSerializerConfig::default().into())).unwrap()
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
        kms_key: Default::default(),
        tags: Default::default(),
        confinement: Default::default(),
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
    use crate::config::ValidatedSink;
    use crate::sinks::aws_cloudwatch_logs::config::CloudwatchLogsSinkConfig;
    use crate::template::{ConfinementConfig, Template};
    use vector_lib::codecs::JsonSerializerConfig;

    #[test]
    fn prepares_valid_config() {
        let mut config = super::default_config(JsonSerializerConfig::default().into());
        config.group_name = "group-{{ file }}".try_into().unwrap();
        config.stream_name = "stream".try_into().unwrap();

        let validated = config.validate().expect("preparation should succeed");
        assert_eq!(validated.group_template.to_string(), "group-{{ file }}");
        assert_eq!(validated.stream_template.to_string(), "stream");
        assert_eq!(validated.batcher_settings.item_limit, 10_000);
    }

    #[test]
    fn test_generate_config() {
        crate::test_util::test_generate_config::<CloudwatchLogsSinkConfig>();
    }

    #[test]
    fn confinement_rejects_unconfined_group_name() {
        let template = Template::try_from("{{ group }}").unwrap();
        let config = ConfinementConfig::default();
        let result = template.confine(&config, "aws_cloudwatch_logs", "group_name");
        assert!(result.is_err());
    }

    #[test]
    fn confinement_opt_out_allows_unconfined_group_name() {
        let template = Template::try_from("{{ group }}").unwrap();
        let config = ConfinementConfig {
            dangerously_allow_unconfined_template_resolution: true,
        };
        let result = template.confine(&config, "aws_cloudwatch_logs", "group_name");
        assert!(result.is_ok());
    }

    #[test]
    fn confinement_allows_prefixed_group_name() {
        let template = Template::try_from("events-{{ env }}").unwrap();
        let config = ConfinementConfig::default();
        let result = template.confine(&config, "aws_cloudwatch_logs", "group_name");
        assert!(result.is_ok());
    }
}
