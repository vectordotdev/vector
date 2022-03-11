use crate::aws::aws_sdk::{create_client, ClientBuilder};
use crate::{
    aws::{auth::AwsAuthentication, region::RegionOrEndpoint},
    codecs::decoding::{DecodingConfig, DeserializerConfig, FramingConfig},
    config::{AcknowledgementsConfig, DataType, Output, SourceConfig, SourceContext},
    serde::{bool_or_struct, default_decoding, default_framing_message_based},
    sources::aws_sqs::source::SqsSource,
};
use aws_sdk_sqs::{Endpoint, Region};
use aws_smithy_client::erase::DynConnector;
use aws_types::credentials::SharedCredentialsProvider;
use serde::{Deserialize, Serialize};
use std::cmp;

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
    pub framing: FramingConfig,
    #[serde(default = "default_decoding")]
    #[derivative(Default(value = "default_decoding()"))]
    pub decoding: DeserializerConfig,
    #[serde(default, deserialize_with = "bool_or_struct")]
    pub acknowledgements: AcknowledgementsConfig,
}

struct SqsClientBuilder;

impl ClientBuilder for SqsClientBuilder {
    type ConfigBuilder = aws_sdk_sqs::config::Builder;
    type Client = aws_sdk_sqs::Client;

    fn create_config_builder(
        credentials_provider: SharedCredentialsProvider,
    ) -> Self::ConfigBuilder {
        aws_sdk_sqs::config::Builder::new().credentials_provider(credentials_provider)
    }

    fn with_endpoint_resolver(
        builder: Self::ConfigBuilder,
        endpoint: Endpoint,
    ) -> Self::ConfigBuilder {
        builder.endpoint_resolver(endpoint)
    }

    fn with_region(builder: Self::ConfigBuilder, region: Region) -> Self::ConfigBuilder {
        builder.region(region)
    }

    fn client_from_conf_conn(
        builder: Self::ConfigBuilder,
        connector: DynConnector,
    ) -> Self::Client {
        Self::Client::from_conf_conn(builder.build(), connector)
    }

    fn client_from_conf(builder: Self::ConfigBuilder) -> Self::Client {
        Self::Client::from_conf(builder.build())
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "aws_sqs")]
impl SourceConfig for AwsSqsConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<crate::sources::Source> {
        let client = self.build_client(&cx).await?;
        let decoder = DecodingConfig::new(self.framing.clone(), self.decoding.clone()).build();
        let acknowledgements = cx.do_acknowledgements(&self.acknowledgements);

        Ok(Box::pin(
            SqsSource {
                client,
                queue_url: self.queue_url.clone(),
                decoder,
                poll_secs: self.poll_secs,
                concurrency: self.client_concurrency,
                acknowledgements,
            }
            .run(cx.out, cx.shutdown),
        ))
    }

    fn outputs(&self) -> Vec<Output> {
        vec![Output::default(DataType::Log)]
    }

    fn source_type(&self) -> &'static str {
        "aws_sqs"
    }

    fn can_acknowledge(&self) -> bool {
        true
    }
}

impl AwsSqsConfig {
    async fn build_client(&self, cx: &SourceContext) -> crate::Result<aws_sdk_sqs::Client> {
        create_client::<SqsClientBuilder>(
            &self.auth,
            self.region.region(),
            self.region.endpoint()?,
            &cx.proxy,
        )
        .await
    }
}

const fn default_poll_secs() -> u32 {
    15
}

fn default_client_concurrency() -> u32 {
    cmp::max(1, num_cpus::get() as u32)
}

impl_generate_config_from_default!(AwsSqsConfig);
