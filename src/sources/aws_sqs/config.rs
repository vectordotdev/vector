use crate::aws::create_client;
use crate::codecs::DecodingConfig;
use crate::common::sqs::SqsClientBuilder;
use crate::tls::TlsOptions;
use crate::{
    aws::{auth::AwsAuthentication, region::RegionOrEndpoint},
    config::{AcknowledgementsConfig, Output, SourceConfig, SourceContext},
    serde::{bool_or_struct, default_decoding, default_framing_message_based},
    sources::aws_sqs::source::SqsSource,
};

use codecs::decoding::{DeserializerConfig, FramingConfig};
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

    // restricted to u32 for safe conversion to i64 later
    #[serde(default = "default_visibility_timeout_secs")]
    #[derivative(Default(value = "default_visibility_timeout_secs()"))]
    pub(super) visibility_timeout_secs: u32,

    #[serde(default = "default_true")]
    #[derivative(Default(value = "default_true()"))]
    pub(super) delete_message: bool,

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
    pub tls: Option<TlsOptions>,
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
                visibility_timeout_secs: self.visibility_timeout_secs,
                delete_message: self.delete_message,
                acknowledgements,
            }
            .run(cx.out, cx.shutdown),
        ))
    }

    fn outputs(&self) -> Vec<Output> {
        vec![Output::default(self.decoding.output_type())]
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
            &self.tls,
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

const fn default_visibility_timeout_secs() -> u32 {
    300
}

const fn default_true() -> bool {
    true
}

impl_generate_config_from_default!(AwsSqsConfig);
