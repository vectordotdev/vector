use std::convert::TryFrom;

use aws_sdk_sqs::Client as SqsClient;
use futures::FutureExt;
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};

use crate::{
    aws::aws_sdk::create_client,
    aws::{AwsAuthentication, RegionOrEndpoint},
    common::sqs::SqsClientBuilder,
    config::{AcknowledgementsConfig, GenerateConfig, Input, ProxyConfig, SinkConfig, SinkContext},
    sinks::util::{encoding::EncodingConfig, TowerRequestConfig},
    template::{Template, TemplateParseError},
    tls::TlsOptions,
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

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct SqsSinkConfig {
    pub queue_url: String,
    #[serde(flatten)]
    pub region: RegionOrEndpoint,
    pub encoding: EncodingConfig<Encoding>,
    pub message_group_id: Option<String>,
    pub message_deduplication_id: Option<String>,
    #[serde(default)]
    pub request: TowerRequestConfig,
    pub tls: Option<TlsOptions>,
    // Deprecated name. Moved to auth.
    pub(super) assume_role: Option<String>,
    #[serde(default)]
    pub auth: AwsAuthentication,
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
#[typetag::serde(name = "aws_sqs")]
impl SinkConfig for SqsSinkConfig {
    async fn build(
        &self,
        cx: SinkContext,
    ) -> crate::Result<(crate::sinks::VectorSink, crate::sinks::Healthcheck)> {
        let client = self.create_client(&cx.proxy).await?;
        let healthcheck = self.clone().healthcheck(client.clone()).boxed();
        let sink = super::sink::SqsSink::new(self.clone(), cx, client)?;
        Ok((
            crate::sinks::VectorSink::from_event_streamsink(sink),
            healthcheck,
        ))
    }

    fn input(&self) -> Input {
        Input::log()
    }

    fn sink_type(&self) -> &'static str {
        "aws_sqs"
    }

    fn acknowledgements(&self) -> Option<&AcknowledgementsConfig> {
        Some(&self.acknowledgements)
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
