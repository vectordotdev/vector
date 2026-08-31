use std::fmt;

use aws_sdk_sqs::Client as SqsClient;
use vector_lib::configurable::configurable_component;

use super::{
    BaseSSSinkConfig, SSRequestBuilder, SSSink, client::SqsMessagePublisher,
    message_deduplication_id, message_group_id,
};
use crate::{
    aws::{RegionOrEndpoint, create_client},
    common::sqs::SqsClientBuilder,
    config::{
        AcknowledgementsConfig, DataType, GenerateConfig, Input, ProxyConfig, SinkConfig,
        SinkContext, ValidatedSink,
    },
    template::UnconfinedTemplate,
};

/// Configuration for the `aws_sqs` sink.
#[configurable_component(sink(
    "aws_sqs",
    "Publish observability events to AWS Simple Queue Service topics."
))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub(super) struct SqsSinkConfig {
    /// The URL of the Amazon SQS queue to which messages are sent.
    #[configurable(validation(format = "uri"))]
    #[configurable(metadata(
        docs::examples = "https://sqs.us-east-2.amazonaws.com/123456789012/MyQueue"
    ))]
    pub(super) queue_url: String,

    #[serde(flatten)]
    pub(super) region: RegionOrEndpoint,

    #[serde(flatten)]
    pub(super) base_config: BaseSSSinkConfig,
}

impl GenerateConfig for SqsSinkConfig {
    fn generate_config() -> serde_json::Value {
        serde_yaml::from_str(indoc::indoc! {
            r#"queue_url: https://sqs.us-east-2.amazonaws.com/123456789012/MyQueue
            region: us-east-2
            encoding:
              codec: json"#,
        })
        .unwrap()
    }
}

impl SqsSinkConfig {
    pub(super) async fn create_client(&self, proxy: &ProxyConfig) -> crate::Result<SqsClient> {
        create_client::<SqsClientBuilder>(
            &SqsClientBuilder {},
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
#[typetag::serde(name = "aws_sqs")]
impl SinkConfig for SqsSinkConfig {
    fn input(&self) -> Input {
        Input::new(self.base_config.encoding.config().input_type() & DataType::Log)
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.base_config.acknowledgements
    }
}

#[derive(Clone)]
pub struct ValidatedSqsSink {
    message_group_id: Option<UnconfinedTemplate>,
    message_deduplication_id: Option<UnconfinedTemplate>,
}

impl fmt::Debug for ValidatedSqsSink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ValidatedSqsSink").finish_non_exhaustive()
    }
}

#[async_trait::async_trait]
impl ValidatedSink for SqsSinkConfig {
    type Validated = ValidatedSqsSink;

    fn validate(&self) -> crate::Result<ValidatedSqsSink> {
        let message_group_id = message_group_id(
            self.base_config.message_group_id.clone(),
            self.queue_url.ends_with(".fifo"),
        )?;
        let message_deduplication_id =
            message_deduplication_id(self.base_config.message_deduplication_id.clone())?;

        Ok(ValidatedSqsSink {
            message_group_id,
            message_deduplication_id,
        })
    }

    async fn build(
        &self,
        validated: &ValidatedSqsSink,
        cx: SinkContext,
    ) -> crate::Result<(crate::sinks::VectorSink, crate::sinks::Healthcheck)> {
        let client = self.create_client(&cx.proxy).await?;
        let publisher = SqsMessagePublisher::new(client.clone(), self.queue_url.clone());
        let healthcheck = Box::pin(healthcheck(client.clone(), self.queue_url.clone()));

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

pub(super) async fn healthcheck(client: SqsClient, queue_url: String) -> crate::Result<()> {
    client
        .get_queue_attributes()
        .queue_url(queue_url)
        .send()
        .await
        .map(|_| ())
        .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vector_lib::codecs::TextSerializerConfig;

    fn test_config(queue_url: &str) -> SqsSinkConfig {
        SqsSinkConfig {
            region: RegionOrEndpoint::with_both("us-east-1", "http://localhost:4566"),
            queue_url: queue_url.to_string(),
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
        let config = test_config("https://sqs.us-east-2.amazonaws.com/123456789012/MyQueue.fifo");
        let err = config.validate().unwrap_err().to_string();
        assert!(
            err.contains("message_group_id"),
            "expected error to mention message_group_id, got: {err}"
        );
    }

    #[test]
    fn validate_produces_validated_state() {
        let config = test_config("https://sqs.us-east-2.amazonaws.com/123456789012/MyQueue");
        config.validate().expect("validation should succeed");
    }
}
