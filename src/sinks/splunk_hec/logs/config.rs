use std::sync::Arc;

use futures_util::FutureExt;
use serde::{Deserialize, Serialize};
use tower::ServiceBuilder;
use vector_core::sink::VectorSink;

use super::{encoder::HecLogsEncoder, request_builder::HecLogsRequestBuilder, sink::HecLogsSink};
use crate::{
    config::{DataType, GenerateConfig, SinkConfig, SinkContext},
    http::HttpClient,
    sinks::{
        splunk_hec::common::{
            acknowledgements::HecClientAcknowledgementsConfig,
            build_healthcheck, build_http_batch_service, create_client, host_key,
            service::{HecService, HttpRequestBuilder},
            SplunkHecDefaultBatchSettings,
        },
        util::{
            encoding::EncodingConfig, http::HttpRetryLogic, BatchConfig, Compression,
            ServiceBuilderExt, TowerRequestConfig,
        },
        Healthcheck,
    },
    template::Template,
    tls::TlsOptions,
};

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct HecLogsSinkConfig {
    // Deprecated name
    #[serde(alias = "token")]
    pub default_token: String,
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
    pub batch: BatchConfig<SplunkHecDefaultBatchSettings>,
    #[serde(default)]
    pub request: TowerRequestConfig,
    pub tls: Option<TlsOptions>,
    #[serde(default)]
    pub acknowledgements: HecClientAcknowledgementsConfig,
    // This settings is relevant only for the `humio_logs` sink and should be left to None everywhere else
    pub timestamp_nanos_key: Option<String>,
}

impl GenerateConfig for HecLogsSinkConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            default_token: "${VECTOR_SPLUNK_HEC_TOKEN}".to_owned(),
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
            acknowledgements: Default::default(),
            timestamp_nanos_key: None,
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "splunk_hec_logs")]
impl SinkConfig for HecLogsSinkConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let client = create_client(&self.tls, cx.proxy())?;
        let healthcheck = build_healthcheck(
            self.endpoint.clone(),
            self.default_token.clone(),
            client.clone(),
        )
        .boxed();
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

impl HecLogsSinkConfig {
    pub fn build_processor(
        &self,
        client: HttpClient,
        cx: SinkContext,
    ) -> crate::Result<VectorSink> {
        let ack_client = if self.acknowledgements.indexer_acknowledgements_enabled {
            Some(client.clone())
        } else {
            None
        };

        let request_builder = HecLogsRequestBuilder {
            encoding: self.encoding.clone(),
            compression: self.compression,
        };

        let request_settings = self.request.unwrap_with(&TowerRequestConfig::default());
        let http_request_builder = Arc::new(HttpRequestBuilder::new(
            self.endpoint.clone(),
            self.default_token.clone(),
            self.compression,
        ));
        let http_service = ServiceBuilder::new()
            .settings(request_settings, HttpRetryLogic)
            .service(build_http_batch_service(
                client,
                Arc::clone(&http_request_builder),
            ));

        let service = HecService::new(
            http_service,
            ack_client,
            http_request_builder,
            self.acknowledgements.clone(),
        );

        let batch_settings = self.batch.into_batcher_settings()?;

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
            timestamp_nanos_key: self.timestamp_nanos_key.clone(),
        };

        Ok(VectorSink::from_event_streamsink(sink))
    }
}

// Add a compatibility alias to avoid breaking existing configs
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct HecSinkCompatConfig {
    #[serde(flatten)]
    config: HecLogsSinkConfig,
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
    use super::HecLogsSinkConfig;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<HecLogsSinkConfig>();
    }
}
