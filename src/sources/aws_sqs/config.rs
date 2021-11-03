use crate::aws::auth::AwsAuthentication;
use crate::aws::rusoto::RegionOrEndpoint;
use crate::codecs::{FramingConfig, ParserConfig};
use crate::config::{DataType, SourceConfig, SourceContext};
use crate::serde::{default_decoding, default_framing_message_based};
use crate::sources::aws_sqs::source::SqsSource;
use aws_sdk_sqs::Endpoint;
use http::Uri;
use serde::{Deserialize, Serialize};
use std::cmp;

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct AwsSqsConfig {
    #[serde(flatten)]
    pub region: RegionOrEndpoint,
    #[serde(default)]
    pub auth: AwsAuthentication,

    pub queue_url: String,
    // restricted to u32 for safe conversion to i64 later
    // #[serde(default = "default_poll_secs")]
    // pub poll_secs: u32,
    //
    // // restricted to u32 for safe conversion to i64 later
    // #[serde(default = "default_visibility_timeout_secs")]
    // pub visibility_timeout_secs: u32,
    //
    // #[serde(default = "default_true")]
    // pub delete_message: bool,
    //
    // // number of tasks spawned for running the SQS/S3 receive loop
    // #[serde(default = "default_client_concurrency")]
    // pub client_concurrency: u32,
    //
    // #[serde(default = "default_framing_message_based")]
    // framing: Box<dyn FramingConfig>,
    // #[serde(default = "default_decoding")]
    // decoding: Box<dyn ParserConfig>,
}

#[async_trait::async_trait]
#[typetag::serde(name = "aws_sqs")]
impl SourceConfig for AwsSqsConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<crate::sources::Source> {
        let mut config_builder = aws_sdk_sqs::config::Builder::new();

        if let Some(endpoint_override) = self.region.endpoint()? {
            config_builder = config_builder.endpoint_resolver(endpoint_override);
        }

        let client = aws_sdk_sqs::Client::from_conf(config_builder.build());

        Ok(Box::pin(SqsSource { client }.run(cx.out, cx.shutdown)))
        // let multiline_config: Option<line_agg::Config> = self
        //     .multiline
        //     .as_ref()
        //     .map(|config| config.try_into())
        //     .transpose()?;
        //
        // match self.strategy {
        //     Strategy::Sqs => Ok(Box::pin(
        //         self.create_sqs_ingestor(multiline_config, &cx.proxy)
        //             .await?
        //             .run(cx.out, cx.shutdown),
        //     )),
        // }
        // todo!()
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

const fn default_visibility_timeout_secs() -> u32 {
    300
}

const fn default_true() -> bool {
    true
}

fn default_client_concurrency() -> u32 {
    cmp::max(1, num_cpus::get() as u32)
}

impl_generate_config_from_default!(AwsSqsConfig);

impl Default for AwsSqsConfig {
    fn default() -> Self {
        todo!()
    }
}
