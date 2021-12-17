use std::cmp;

use serde::{Deserialize, Serialize};

use crate::{
    aws::{auth::AwsAuthentication, region::RegionOrEndpoint},
    codecs::decoding::{DecodingConfig, DeserializerConfig, FramingConfig},
    config::{AcknowledgementsConfig, DataType, SourceConfig, SourceContext},
    serde::{bool_or_struct, default_decoding, default_framing_message_based},
    sources::aws_sqs::source::SqsSource,
};

#[derive(Deserialize, Serialize, Derivative, Debug, Clone)]
#[derivative(Default)]
#[serde(deny_unknown_fields)]
pub struct AwsSqsConfig {
    #[serde(flatten)]
    pub region: RegionOrEndpoint,
    #[serde(default)]
    pub auth: AwsAuthentication,

    pub queue_url: String,

    #[serde(default = "default_poll_secs")]
    #[derivative(Default(value = "default_poll_secs()"))]
    pub poll_secs: u32,

    // number of concurrent tasks spawned for receiving/processing SQS messages
    #[serde(default = "default_client_concurrency")]
    #[derivative(Default(value = "default_client_concurrency()"))]
    pub client_concurrency: u32,

    #[serde(default = "default_framing_message_based")]
    #[derivative(Default(value = "default_framing_message_based()"))]
    pub framing: Box<dyn FramingConfig>,
    #[serde(default = "default_decoding")]
    #[derivative(Default(value = "default_decoding()"))]
    pub decoding: Box<dyn DeserializerConfig>,
    #[serde(default, deserialize_with = "bool_or_struct")]
    pub acknowledgements: AcknowledgementsConfig,
}

#[async_trait::async_trait]
#[typetag::serde(name = "aws_sqs")]
impl SourceConfig for AwsSqsConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<crate::sources::Source> {
        let mut config_builder = aws_sdk_sqs::config::Builder::new()
            .credentials_provider(self.auth.credentials_provider().await);

        if let Some(endpoint_override) = self.region.endpoint()? {
            config_builder = config_builder.endpoint_resolver(endpoint_override);
        }
        if let Some(region) = self.region.region() {
            config_builder = config_builder.region(region);
        }

        let client = aws_sdk_sqs::Client::from_conf(config_builder.build());
        let decoder = DecodingConfig::new(self.framing.clone(), self.decoding.clone()).build()?;

        Ok(Box::pin(
            SqsSource {
                client,
                queue_url: self.queue_url.clone(),
                decoder,
                poll_secs: self.poll_secs,
                concurrency: self.client_concurrency,
                acknowledgements: self.acknowledgements.enabled,
            }
            .run(cx.out, cx.shutdown),
        ))
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn source_type(&self) -> &'static str {
        "aws_sqs"
    }
}

const fn default_poll_secs() -> u32 {
    15
}

fn default_client_concurrency() -> u32 {
    cmp::max(1, num_cpus::get() as u32)
}

impl_generate_config_from_default!(AwsSqsConfig);
