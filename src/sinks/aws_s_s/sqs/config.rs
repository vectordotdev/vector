use aws_sdk_sqs::Client as SqsClient;

use crate::aws::RegionOrEndpoint;

use crate::config::{
    AcknowledgementsConfig, DataType, GenerateConfig, Input, ProxyConfig, SinkConfig, SinkContext,
};
use vector_lib::configurable::configurable_component;

use super::{
    client::SqsMessagePublisher, message_deduplication_id, message_group_id, BaseSSSinkConfig,
    SSRequestBuilder, SSSink,
};
use crate::{aws::create_client, common::sqs::SqsClientBuilder};

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
    pub(super) async fn create_client(&self, proxy: &ProxyConfig) -> crate::Result<SqsClient> {
        create_client::<SqsClientBuilder>(
            &self.base_config.auth,
            self.region.region(),
            self.region.endpoint(),
            proxy,
            &self.base_config.tls,
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

        let healthcheck = Box::pin(healthcheck(client.clone(), self.queue_url.clone()));
        let message_group_id = message_group_id(
            self.base_config.message_group_id.clone(),
            self.queue_url.ends_with(".fifo"),
        );
        let message_deduplication_id =
            message_deduplication_id(self.base_config.message_deduplication_id.clone());

        let sink = SSSink::new(
            SSRequestBuilder::new(
                message_group_id?,
                message_deduplication_id?,
                self.base_config.encoding.clone(),
            )?,
            self.base_config.request,
            publisher,
        )?;
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

pub(super) async fn healthcheck(client: SqsClient, queue_url: String) -> crate::Result<()> {
    client
        .get_queue_attributes()
        .queue_url(queue_url)
        .send()
        .await
        .map(|_| ())
        .map_err(Into::into)
}
