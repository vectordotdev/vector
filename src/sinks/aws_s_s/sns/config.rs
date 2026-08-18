use std::fmt;

use aws_sdk_sns::Client as SnsClient;
use vector_lib::configurable::configurable_component;

use super::{
    BaseSSSinkConfig, SSRequestBuilder, SSSink, client::SnsMessagePublisher,
    message_deduplication_id, message_group_id,
};
use crate::{
    aws::{ClientBuilder, RegionOrEndpoint, create_client},
    config::{
        AcknowledgementsConfig, DataType, GenerateConfig, Input, ProxyConfig, SinkConfig,
        SinkContext, ValidatedSink,
    },
    template::UnconfinedTemplate,
};

/// Configuration for the `aws_sns` sink.
#[configurable_component(sink(
    "aws_sns",
    "Publish observability events to AWS Simple Notification Service topics."
))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub(super) struct SnsSinkConfig {
    /// The ARN of the Amazon SNS topic to which messages are sent.
    #[configurable(validation(format = "uri"))]
    #[configurable(metadata(docs::examples = "arn:aws:sns:us-east-2:123456789012:MyTopic"))]
    #[configurable(metadata(
        docs::examples = "arn:aws:sns:us-east-2:123456789012:FifoTopic.fifo"
    ))]
    pub(super) topic_arn: String,

    #[serde(flatten)]
    pub(super) region: RegionOrEndpoint,

    #[serde(flatten)]
    pub(super) base_config: BaseSSSinkConfig,
}

impl GenerateConfig for SnsSinkConfig {
    fn generate_config() -> serde_json::Value {
        serde_yaml::from_str(indoc::indoc! {
            r#"topic_arn: arn:aws:sns:us-east-2:123456789012:MyTopic
            region: us-east-2
            encoding:
              codec: json"#,
        })
        .unwrap()
    }
}

impl SnsSinkConfig {
    pub(super) async fn create_client(&self, proxy: &ProxyConfig) -> crate::Result<SnsClient> {
        create_client::<SnsClientBuilder>(
            &SnsClientBuilder {},
            &self.base_config.auth,
            self.region.region(),
            self.region.endpoint(),
            proxy,
            self.base_config.tls.as_ref(),
            None,
        )
        .await
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "aws_sns")]
impl SinkConfig for SnsSinkConfig {
    fn input(&self) -> Input {
        Input::new(self.base_config.encoding.config().input_type() & DataType::Log)
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.base_config.acknowledgements
    }
}

#[derive(Clone)]
pub struct ValidatedSnsSink {
    message_group_id: Option<UnconfinedTemplate>,
    message_deduplication_id: Option<UnconfinedTemplate>,
}

impl fmt::Debug for ValidatedSnsSink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ValidatedSnsSink").finish_non_exhaustive()
    }
}

#[async_trait::async_trait]
impl ValidatedSink for SnsSinkConfig {
    type Validated = ValidatedSnsSink;

    fn validate(&self) -> crate::Result<ValidatedSnsSink> {
        let message_group_id = message_group_id(
            self.base_config.message_group_id.clone(),
            self.topic_arn.ends_with(".fifo"),
        )?;
        let message_deduplication_id =
            message_deduplication_id(self.base_config.message_deduplication_id.clone())?;

        Ok(ValidatedSnsSink {
            message_group_id,
            message_deduplication_id,
        })
    }

    async fn build(
        &self,
        validated: &ValidatedSnsSink,
        cx: SinkContext,
    ) -> crate::Result<(crate::sinks::VectorSink, crate::sinks::Healthcheck)> {
        let client = self.create_client(&cx.proxy).await?;
        let publisher = SnsMessagePublisher::new(client.clone(), self.topic_arn.clone());
        let healthcheck = Box::pin(healthcheck(client.clone(), self.topic_arn.clone()));

        let request_builder = SSRequestBuilder::new(
            validated.message_group_id.clone(),
            validated.message_deduplication_id.clone(),
            self.base_config.encoding.clone(),
        )?;
        let sink = SSSink::new(request_builder, self.base_config.request, publisher)?;
        Ok((
            crate::sinks::VectorSink::from_event_streamsink(sink),
            healthcheck,
        ))
    }
}

pub(super) struct SnsClientBuilder;

impl ClientBuilder for SnsClientBuilder {
    type Client = aws_sdk_sns::client::Client;

    fn build(&self, config: &aws_types::SdkConfig) -> Self::Client {
        aws_sdk_sns::client::Client::new(config)
    }
}

pub(super) async fn healthcheck(client: SnsClient, topic_arn: String) -> crate::Result<()> {
    client
        .get_topic_attributes()
        .topic_arn(topic_arn.clone())
        .send()
        .await
        .map(|_| ())
        .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vector_lib::codecs::TextSerializerConfig;

    fn test_config(topic_arn: &str) -> SnsSinkConfig {
        SnsSinkConfig {
            region: RegionOrEndpoint::with_both("us-east-1", "http://localhost:4566"),
            topic_arn: topic_arn.to_string(),
            base_config: BaseSSSinkConfig {
                encoding: TextSerializerConfig::default().into(),
                message_group_id: None,
                message_deduplication_id: None,
                request: Default::default(),
                tls: Default::default(),
                assume_role: None,
                auth: Default::default(),
                acknowledgements: Default::default(),
            },
        }
    }

    #[test]
    fn validate_rejects_fifo_without_message_group_id() {
        let config = test_config("arn:aws:sns:us-east-2:123456789012:MyTopic.fifo");
        let err = config.validate().unwrap_err().to_string();
        assert!(
            err.contains("message_group_id"),
            "expected error to mention message_group_id, got: {err}"
        );
    }

    #[test]
    fn validate_produces_validated_state() {
        let config = test_config("arn:aws:sns:us-east-2:123456789012:MyTopic");
        config.validate().expect("validation should succeed");
    }
}
