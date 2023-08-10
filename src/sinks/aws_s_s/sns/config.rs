use aws_sdk_sns::Client as SnsClient;

use crate::config::{
    AcknowledgementsConfig, DataType, GenerateConfig, Input, ProxyConfig, SinkConfig, SinkContext,
};
use futures::FutureExt;
use vector_config::configurable_component;

use super::{client::SnsMessagePublisher, BaseSSSinkConfig, ConfigWithIds, SSSink};
use crate::aws::create_client;
use crate::aws::ClientBuilder;

/// Configuration for the `aws_sns` sink.
#[configurable_component(sink(
    "aws_sns",
    "Publish observability events to AWS Simple Queue Service topics."
))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct SnsSinkConfig {
    /// The ARN of the Amazon SNS topc to which messages are sent.
    #[configurable(validation(format = "uri"))]
    // #TODO: Fix example
    #[configurable(metadata(
        docs::examples = "https://sqs.us-east-2.amazonaws.com/123456789012/MyQueue"
    ))]
    pub topic_arn: String,

    #[serde(flatten)]
    pub base_config: BaseSSSinkConfig,
}

impl GenerateConfig for SnsSinkConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"queue_url = "https://sqs.us-east-2.amazonaws.com/123456789012/MyQueue"
            region = "us-east-2"
            encoding.codec = "json""#,
        )
        .unwrap()
    }
}

impl SnsSinkConfig {
    pub async fn healthcheck(self, client: SnsClient) -> crate::Result<()> {
        client
            .get_topic_attributes()
            .topic_arn(self.topic_arn.clone())
            .send()
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

    pub async fn create_client(&self, proxy: &ProxyConfig) -> crate::Result<SnsClient> {
        create_client::<SnsClientBuilder>(
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
#[typetag::serde(name = "aws_sns")]
impl SinkConfig for SnsSinkConfig {
    async fn build(
        &self,
        cx: SinkContext,
    ) -> crate::Result<(crate::sinks::VectorSink, crate::sinks::Healthcheck)> {
        let client = self.create_client(&cx.proxy).await?;

        let publisher = SnsMessagePublisher::new(client.clone(), self.topic_arn.clone());

        let healthcheck = self.clone().healthcheck(client.clone()).boxed();
        let config = ConfigWithIds {
            base_config: self.base_config.clone(),
            fifo: self.topic_arn.ends_with(".fifo"),
        };

        let sink = SSSink::new(config.clone(), publisher)?;
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

pub(crate) struct SnsClientBuilder;

impl ClientBuilder for SnsClientBuilder {
    type Config = aws_sdk_sns::config::Config;
    type Client = aws_sdk_sns::client::Client;
    type DefaultMiddleware = aws_sdk_sns::middleware::DefaultMiddleware;

    fn default_middleware() -> Self::DefaultMiddleware {
        aws_sdk_sns::middleware::DefaultMiddleware::new()
    }

    fn build(client: aws_smithy_client::Client, config: &aws_types::SdkConfig) -> Self::Client {
        aws_sdk_sns::client::Client::with_config(client, config.into())
    }
}
