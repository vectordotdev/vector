use std::convert::TryFrom;

use aws_sdk_sqs::Client as SqsClient;
use futures::FutureExt;
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use vector_config::configurable_component;

use crate::{
    aws::{create_client, AwsAuthentication, RegionOrEndpoint},
    codecs::EncodingConfig,
    common::sqs::SqsClientBuilder,
    config::{
        AcknowledgementsConfig, DataType, GenerateConfig, Input, ProxyConfig, SinkConfig,
        SinkContext,
    },
    sinks::util::TowerRequestConfig,
    template::{Template, TemplateParseError},
    tls::TlsConfig,
};

#[derive(Debug, Snafu)]
pub(super) enum BuildError {
    #[snafu(display("`message_group_id` should be defined for FIFO queue."))]
    MessageGroupIdMissing,
    #[snafu(display("`message_group_id` is not allowed with non-FIFO queue."))]
    MessageGroupIdNotAllowed,
    #[snafu(display("invalid topic template: {}", source))]
    TopicTemplate { source: TemplateParseError },
    #[snafu(display("invalid message_deduplication_id template: {}", source))]
    MessageDeduplicationIdTemplate { source: TemplateParseError },
}

/// Configuration for the `aws_sqs` sink.
#[configurable_component(sink("aws_sqs"))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct SqsSinkConfig {
    /// The URL of the Amazon SQS queue to which messages are sent.
    #[configurable(validation(format = "uri"))]
    #[configurable(metadata(
        docs::examples = "https://sqs.us-east-2.amazonaws.com/123456789012/MyQueue"
    ))]
    pub queue_url: String,

    #[serde(flatten)]
    pub region: RegionOrEndpoint,

    #[configurable(derived)]
    pub encoding: EncodingConfig,

    /// The tag that specifies that a message belongs to a specific message group.
    ///
    /// Can be applied only to FIFO queues.
    #[configurable(metadata(docs::examples = "vector"))]
    #[configurable(metadata(docs::examples = "vector-%Y-%m-%d"))]
    pub message_group_id: Option<String>,

    /// The message deduplication ID value to allow AWS to identify duplicate messages.
    ///
    /// This value is a template which should result in a unique string for each event. See the [AWS
    /// documentation][deduplication_id_docs] for more about how AWS does message deduplication.
    ///
    /// [deduplication_id_docs]: https://docs.aws.amazon.com/AWSSimpleQueueService/latest/SQSDeveloperGuide/using-messagededuplicationid-property.html
    #[configurable(metadata(docs::examples = "{{ transaction_id }}"))]
    pub message_deduplication_id: Option<String>,

    #[configurable(derived)]
    #[serde(default)]
    pub request: TowerRequestConfig,

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
    pub(super) acknowledgements: AcknowledgementsConfig,
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone, Derivative)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    Text,
    Json,
}

impl GenerateConfig for SqsSinkConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"queue_url = "https://sqs.us-east-2.amazonaws.com/123456789012/MyQueue"
            region = "us-east-2"
            encoding.codec = "json""#,
        )
        .unwrap()
    }
}

#[async_trait::async_trait]
impl SinkConfig for SqsSinkConfig {
    async fn build(
        &self,
        cx: SinkContext,
    ) -> crate::Result<(crate::sinks::VectorSink, crate::sinks::Healthcheck)> {
        let client = self.create_client(&cx.proxy).await?;
        let healthcheck = self.clone().healthcheck(client.clone()).boxed();
        let sink = super::sink::SqsSink::new(self.clone(), client)?;
        Ok((
            crate::sinks::VectorSink::from_event_streamsink(sink),
            healthcheck,
        ))
    }

    fn input(&self) -> Input {
        Input::new(self.encoding.config().input_type() & DataType::Log)
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

impl SqsSinkConfig {
    pub async fn healthcheck(self, client: SqsClient) -> crate::Result<()> {
        client
            .get_queue_attributes()
            .queue_url(self.queue_url.clone())
            .send()
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

    pub async fn create_client(&self, proxy: &ProxyConfig) -> crate::Result<SqsClient> {
        create_client::<SqsClientBuilder>(
            &self.auth,
            self.region.region(),
            self.region.endpoint()?,
            proxy,
            &self.tls,
            true,
        )
        .await
    }

    pub fn message_group_id(&self) -> crate::Result<Option<Template>> {
        let fifo = self.queue_url.ends_with(".fifo");
        match (self.message_group_id.as_ref(), fifo) {
            (Some(value), true) => Ok(Some(
                Template::try_from(value.clone()).context(TopicTemplateSnafu)?,
            )),
            (Some(_), false) => Err(Box::new(BuildError::MessageGroupIdNotAllowed)),
            (None, true) => Err(Box::new(BuildError::MessageGroupIdMissing)),
            (None, false) => Ok(None),
        }
    }

    pub fn message_deduplication_id(&self) -> crate::Result<Option<Template>> {
        Ok(self
            .message_deduplication_id
            .clone()
            .map(Template::try_from)
            .transpose()?)
    }
}
