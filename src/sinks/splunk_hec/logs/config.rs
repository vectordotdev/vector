use futures_util::FutureExt;
use tower::ServiceBuilder;
use vector_core::{sink::VectorSink, transform::DataType};

use crate::{config::{GenerateConfig, SinkConfig, SinkContext}, http::HttpClient, sinks::{Healthcheck, splunk_hec::common::{build_healthcheck, create_client, host_key, retry::HecRetryLogic}, util::{
            encoding::EncodingConfig, BatchConfig, BatchSettings, Buffer, Compression,
            ServiceBuilderExt, TowerRequestConfig,
        }}, template::Template, tls::TlsOptions};

use serde::{Deserialize, Serialize};

use super::{
    encoder::HecLogsEncoder,
    request_builder::HecLogsRequestBuilder,
    service::{HecLogsService, HttpRequestBuilder},
    sink::HecLogsSink,
};

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct HecSinkLogsConfig {
    pub token: String,
    // Deprecated name
    #[serde(alias = "host")]
    pub endpoint: String,
    #[serde(default = "host_key")]
    pub host_key: String,
    #[serde(default)]
    pub indexed_fields: Vec<String>,
    pub index: Option<Template>,
    pub sourcetype: Option<Template>,
    pub source: Option<Template>,
    pub encoding: EncodingConfig<HecLogsEncoder>,
    #[serde(default)]
    pub compression: Compression,
    #[serde(default)]
    pub batch: BatchConfig,
    #[serde(default)]
    pub request: TowerRequestConfig,
    pub tls: Option<TlsOptions>,
}

impl GenerateConfig for HecSinkLogsConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            token: "${VECTOR_SPLUNK_HEC_TOKEN}".to_owned(),
            endpoint: "endpoint".to_owned(),
            host_key: host_key(),
            indexed_fields: vec![],
            index: None,
            sourcetype: None,
            source: None,
            encoding: HecLogsEncoder::Text.into(),
            compression: Compression::default(),
            batch: BatchConfig::default(),
            request: TowerRequestConfig::default(),
            tls: None,
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "splunk_hec_logs")]
impl SinkConfig for HecSinkLogsConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let client = create_client(&self.tls, cx.proxy())?;
        let healthcheck =
            build_healthcheck(self.endpoint.clone(), self.token.clone(), client.clone()).boxed();
        let sink = self.build_processor(client, cx)?;

        Ok((sink, healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "splunk_hec_logs"
    }
}

impl HecSinkLogsConfig {
    pub fn build_processor(
        &self,
        client: HttpClient,
        cx: SinkContext,
    ) -> crate::Result<VectorSink> {
        let request_builder = HecLogsRequestBuilder {
            encoding: self.encoding.clone(),
            compression: self.compression,
        };

        let request_settings = self.request.unwrap_with(&TowerRequestConfig::default());
        let http_request_builder = HttpRequestBuilder {
            endpoint: self.endpoint.clone(),
            token: self.token.clone(),
            compression: self.compression,
        };
        let service = ServiceBuilder::new()
            .settings(request_settings, HecRetryLogic)
            .service(HecLogsService::new(client, http_request_builder));

        let batch_settings = BatchSettings::<Buffer>::default()
            .bytes(1_000_000)
            .timeout(1)
            .parse_config(self.batch)?
            .into_batcher_settings()?;

        let sink = HecLogsSink {
            service,
            request_builder,
            context: cx,
            batch_settings,
            sourcetype: self.sourcetype.clone(),
            source: self.source.clone(),
            index: self.index.clone(),
            indexed_fields: self.indexed_fields.clone(),
            host: self.host_key.clone(),
        };

        Ok(VectorSink::Stream(Box::new(sink)))
    }
}

// Add a compatibility alias to avoid breaking existing configs
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct HecSinkCompatConfig {
    #[serde(flatten)]
    config: HecSinkLogsConfig,
}

#[async_trait::async_trait]
#[typetag::serde(name = "splunk_hec")]
impl SinkConfig for HecSinkCompatConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        self.config.build(cx).await
    }

    fn input_type(&self) -> DataType {
        self.config.input_type()
    }

    fn sink_type(&self) -> &'static str {
        "splunk_hec"
    }
}

#[cfg(test)]
mod tests {
    use super::HecSinkLogsConfig;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<HecSinkLogsConfig>();
    }
}
