use serde::{Deserialize, Serialize};

use crate::template::Template;
use vector_core::sink::VectorSink;
use crate::config::SinkConfig;

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct HecMetricsSinkConfig {
    pub default_namespace: Option<String>,
    pub token: String,
    pub endpoint: String,
    #[serde(default = "host_key")]
    pub host_key: String,
    pub index: Option<Template>,
    pub sourcetype: Option<Template>,
    pub source: Option<Template>,
    #[serde(default)]
    pub compression: Compression,
    #[serde(default)]
    pub batch: BatchConfig,
    #[serde(default)]
    pub request: TowerRequestConfig,
    pub tls: Option<TlsOptions>,
}

impl GenerateConfig for HecMetricsSinkConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            default_namespace: None,
            token: "${VECTOR_SPLUNK_HEC_TOKEN}".to_owned(),
            endpoint: "http://localhost:8088".to_owned(),
            host_key: host_key(),
            index: None,
            sourcetype: None,
            source: None,
            compression: Compression::default(),
            batch: BatchConfig::default(),
            request: TowerRequestConfig::default(),
            tls: None,
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "splunk_hec_metrics")]
impl SinkConfig for HecMetricsSinkConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let client = create_client(&self.tls, cx.proxy())?;
        let healthcheck =
            build_healthcheck(self.endpoint.clone(), self.token.clone(), client.clone()).boxed();
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
        // Create a request builder 
        let request_builder = todo!();

        // Create a service for making HTTP requests 
        let service = todo!();

        // Create a HEC metrics sink
        let sink = todo!();

        Ok(VectorSink::Stream(Box::new(sink)))
    }
}