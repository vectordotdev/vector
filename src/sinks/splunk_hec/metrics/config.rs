use std::sync::Arc;

use futures_util::FutureExt;
use serde::{Deserialize, Serialize};
use tower::ServiceBuilder;
use vector_core::sink::VectorSink;

use super::{request_builder::HecMetricsRequestBuilder, sink::HecMetricsSink};
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
            http::HttpRetryLogic, BatchConfig, Compression, ServiceBuilderExt, TowerRequestConfig,
        },
        Healthcheck,
    },
    template::Template,
    tls::TlsOptions,
};

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct HecMetricsSinkConfig {
    pub default_namespace: Option<String>,
    // Deprecated name
    #[serde(alias = "token")]
    pub default_token: String,
    pub endpoint: String,
    #[serde(default = "crate::sinks::splunk_hec::common::host_key")]
    pub host_key: String,
    pub index: Option<Template>,
    pub sourcetype: Option<Template>,
    pub source: Option<Template>,
    #[serde(default)]
    pub compression: Compression,
    #[serde(default)]
    pub batch: BatchConfig<SplunkHecDefaultBatchSettings>,
    #[serde(default)]
    pub request: TowerRequestConfig,
    pub tls: Option<TlsOptions>,
    #[serde(default)]
    pub acknowledgements: HecClientAcknowledgementsConfig,
}

impl GenerateConfig for HecMetricsSinkConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            default_namespace: None,
            default_token: "${VECTOR_SPLUNK_HEC_TOKEN}".to_owned(),
            endpoint: "http://localhost:8088".to_owned(),
            host_key: host_key(),
            index: None,
            sourcetype: None,
            source: None,
            compression: Compression::default(),
            batch: BatchConfig::default(),
            request: TowerRequestConfig::default(),
            tls: None,
            acknowledgements: Default::default(),
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "splunk_hec_metrics")]
impl SinkConfig for HecMetricsSinkConfig {
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
        DataType::Metric
    }

    fn sink_type(&self) -> &'static str {
        "splunk_hec_metrics"
    }
}

impl HecMetricsSinkConfig {
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

        let request_builder = HecMetricsRequestBuilder {
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

        let sink = HecMetricsSink {
            context: cx,
            service,
            batch_settings,
            request_builder,
            sourcetype: self.sourcetype.clone(),
            source: self.source.clone(),
            index: self.index.clone(),
            host: self.host_key.clone(),
            default_namespace: self.default_namespace.clone(),
        };

        Ok(VectorSink::from_event_streamsink(sink))
    }
}
