use aws_sdk_sqs::Client as SqsClient;

use crate::config::{
    AcknowledgementsConfig, DataType, GenerateConfig, Input, ProxyConfig, SinkConfig, SinkContext,
};
use futures::FutureExt;
use vector_config::configurable_component;

use super::{client::SqsMessagePublisher, BaseSSSinkConfig, ConfigWithIds, SqsSink};
use crate::{aws::create_client, common::sqs::SqsClientBuilder};

/// Configuration for the `aws_sqs` sink.
#[configurable_component(sink(
    "aws_sqs",
    "Publish observability events to AWS Simple Queue Service topics."
))]
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
    pub base_config: BaseSSSinkConfig,
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
            &self.base_config.auth,
            self.base_config.region.region(),
            self.base_config.region.endpoint(),
            proxy,
            &self.base_config.tls,
            true,
        )
        .await
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

        let publisher = SqsMessagePublisher::new(client.clone(), self.queue_url.clone());

        let healthcheck = self.clone().healthcheck(client.clone()).boxed();
        let config = ConfigWithIds {
            base_config: self.base_config.clone(),
            fifo: self.queue_url.ends_with(".fifo"),
        };

        let sink = SqsSink::new(config.clone(), publisher)?;
        Ok((
            crate::sinks::VectorSink::from_event_streamsink(sink),
            healthcheck,
        ))
    }

    fn input(&self) -> Input {
        Input::new(self.base_config.encoding.config().input_type() & DataType::Log)
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.base_config.acknowledgements
    }
}
