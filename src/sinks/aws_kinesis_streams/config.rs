use std::convert::TryInto;
use rusoto_core::RusotoError;
use rusoto_kinesis::{DescribeStreamInput, Kinesis, KinesisClient};
use crate::config::{DataType, GenerateConfig, ProxyConfig, SinkConfig, SinkContext};
use crate::rusoto::{AwsAuthentication, RegionOrEndpoint};
use crate::sinks::aws_kinesis_streams::service::KinesisService;
use crate::sinks::util::{BatchConfig, Compression, TowerRequestConfig};
use crate::sinks::util::encoding::{EncodingConfig, StandardEncodings};
use futures::FutureExt;

#[derive(Debug, Snafu)]
enum HealthcheckError {
    #[snafu(display("DescribeStream failed: {}", source))]
    DescribeStreamFailed {
        source: RusotoError<rusoto_kinesis::DescribeStreamError>,
    },
    #[snafu(display("Stream names do not match, got {}, expected {}", name, stream_name))]
    StreamNamesMismatch { name: String, stream_name: String },
    #[snafu(display(
    "Stream returned does not contain any streams that match {}",
    stream_name
    ))]
    NoMatchingStreamName { stream_name: String },
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct KinesisSinkConfig {
    pub stream_name: String,
    pub partition_key_field: Option<String>,
    #[serde(flatten)]
    pub region: RegionOrEndpoint,
    pub encoding: EncodingConfig<StandardEncodings>,
    #[serde(default)]
    pub compression: Compression,
    #[serde(default)]
    pub batch: BatchConfig,
    #[serde(default)]
    pub request: TowerRequestConfig,
    // Deprecated name. Moved to auth.
    assume_role: Option<String>,
    #[serde(default)]
    pub auth: AwsAuthentication,
}

impl KinesisSinkConfig {
    async fn healthcheck(self, client: KinesisClient) -> crate::Result<()> {
        let stream_name = self.stream_name;

        let req = client.describe_stream(DescribeStreamInput {
            stream_name: stream_name.clone(),
            exclusive_start_shard_id: None,
            limit: Some(1),
        });

        match req.await {
            Ok(resp) => {
                let name = resp.stream_description.stream_name;
                if name == stream_name {
                    Ok(())
                } else {
                    Err(HealthcheckError::StreamNamesMismatch { name, stream_name }.into())
                }
            }
            Err(source) => Err(HealthcheckError::DescribeStreamFailed { source }.into()),
        }
    }

    fn create_client(&self, proxy: &ProxyConfig) -> crate::Result<KinesisClient> {
        let region = (&self.region).try_into()?;

        let client = rusoto::client(proxy)?;
        let creds = self.auth.build(&region, self.assume_role.clone())?;

        let client = rusoto_core::Client::new_with_encoding(creds, client, self.compression.into());
        Ok(KinesisClient::new_with_client(client, region))
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "sinks.aws_kinesis_streams")]
impl SinkConfig for KinesisSinkConfig {
    async fn build(
        &self,
        cx: SinkContext,
    ) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
        let client = self.create_client(&cx.proxy)?;
        let healthcheck = self.clone().healthcheck(client.clone()).boxed();
        // let sink = KinesisService::new(self.clone(), client, cx)?;
        let sink = KinesisSink {

        };
        Ok((super::VectorSink::Stream(Box::new(sink)), healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "sinks.aws_kinesis_streams"
    }
}

impl GenerateConfig for KinesisSinkConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"region = "us-east-1"
            stream_name = "my-stream"
            encoding.codec = "json""#,
        )
            .unwrap()
    }
}
