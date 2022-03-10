use aws_sdk_cloudwatchlogs::{Endpoint, Region};
use std::num::NonZeroU64;

use futures::FutureExt;
use serde::{Deserialize, Serialize};
use vector_core::config::log_schema;

use crate::aws::aws_sdk::{create_client, ClientBuilder};
use crate::{
    aws::{AwsAuthentication, RegionOrEndpoint},
    config::{AcknowledgementsConfig, GenerateConfig, Input, ProxyConfig, SinkConfig, SinkContext},
    sinks::{
        aws_cloudwatch_logs::{
            healthcheck::healthcheck, request_builder::CloudwatchRequestBuilder,
            retry::CloudwatchRetryLogic, service::CloudwatchLogsPartitionSvc, sink::CloudwatchSink,
        },
        util::{
            encoding::{EncodingConfig, StandardEncodings},
            BatchConfig, Compression, SinkBatchSettings, TowerRequestConfig,
        },
        Healthcheck, VectorSink,
    },
    template::Template,
    tls::TlsOptions,
};
use aws_sdk_cloudwatchlogs::Client as CloudwatchLogsClient;
use aws_smithy_client::erase::DynConnector;
use aws_types::credentials::SharedCredentialsProvider;

pub struct CloudwatchLogsClientBuilder;

impl ClientBuilder for CloudwatchLogsClientBuilder {
    type ConfigBuilder = aws_sdk_cloudwatchlogs::config::Builder;
    type Client = CloudwatchLogsClient;

    fn create_config_builder(
        credentials_provider: SharedCredentialsProvider,
    ) -> Self::ConfigBuilder {
        aws_sdk_cloudwatchlogs::config::Builder::new().credentials_provider(credentials_provider)
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

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct CloudwatchLogsSinkConfig {
    pub group_name: Template,
    pub stream_name: Template,
    #[serde(flatten)]
    pub region: RegionOrEndpoint,
    pub encoding: EncodingConfig<StandardEncodings>,
    pub create_missing_group: Option<bool>,
    pub create_missing_stream: Option<bool>,
    #[serde(default)]
    pub compression: Compression,
    #[serde(default)]
    pub batch: BatchConfig<CloudwatchLogsDefaultBatchSettings>,
    #[serde(default)]
    pub request: TowerRequestConfig,
    pub tls: Option<TlsOptions>,
    // Deprecated name. Moved to auth.
    pub assume_role: Option<String>,
    #[serde(default)]
    pub auth: AwsAuthentication,
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
}

impl CloudwatchLogsSinkConfig {
    pub async fn create_client(&self, proxy: &ProxyConfig) -> crate::Result<CloudwatchLogsClient> {
        create_client::<CloudwatchLogsClientBuilder>(
            &self.auth,
            self.region.region(),
            self.region.endpoint()?,
            proxy,
        )
        .await
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "aws_cloudwatch_logs")]
impl SinkConfig for CloudwatchLogsSinkConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let batcher_settings = self.batch.into_batcher_settings()?;
        let request = self.request.unwrap_with(&TowerRequestConfig::default());
        let client = self.create_client(cx.proxy()).await?;
        let svc = request.service(
            CloudwatchRetryLogic::new(),
            CloudwatchLogsPartitionSvc::new(self.clone(), client.clone()),
        );
        let encoding = self.encoding.clone();
        let healthcheck = healthcheck(self.clone(), client).boxed();
        let sink = CloudwatchSink {
            batcher_settings,
            request_builder: CloudwatchRequestBuilder {
                group_template: self.group_name.clone(),
                stream_template: self.stream_name.clone(),
                log_schema: log_schema().clone(),
                encoding,
            },
            acker: cx.acker(),
            service: svc,
        };

        Ok((VectorSink::from_event_streamsink(sink), healthcheck))
    }

    fn input(&self) -> Input {
        Input::log()
    }

    fn sink_type(&self) -> &'static str {
        "aws_cloudwatch_logs"
    }

    fn acknowledgements(&self) -> Option<&AcknowledgementsConfig> {
        Some(&self.acknowledgements)
    }
}

impl GenerateConfig for CloudwatchLogsSinkConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(default_config(StandardEncodings::Json)).unwrap()
    }
}

fn default_config(e: StandardEncodings) -> CloudwatchLogsSinkConfig {
    CloudwatchLogsSinkConfig {
        encoding: e.into(),
        group_name: Default::default(),
        stream_name: Default::default(),
        region: Default::default(),

        create_missing_group: Default::default(),
        create_missing_stream: Default::default(),
        compression: Default::default(),
        batch: Default::default(),
        request: Default::default(),
        tls: Default::default(),
        assume_role: Default::default(),
        auth: Default::default(),
        acknowledgements: Default::default(),
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CloudwatchLogsDefaultBatchSettings;

impl SinkBatchSettings for CloudwatchLogsDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = Some(10_000);
    const MAX_BYTES: Option<usize> = Some(1_048_576);
    const TIMEOUT_SECS: NonZeroU64 = unsafe { NonZeroU64::new_unchecked(1) };
}

#[cfg(test)]
mod tests {
    use crate::sinks::aws_cloudwatch_logs::config::CloudwatchLogsSinkConfig;

    #[test]
    fn test_generate_config() {
        crate::test_util::test_generate_config::<CloudwatchLogsSinkConfig>();
    }
}
