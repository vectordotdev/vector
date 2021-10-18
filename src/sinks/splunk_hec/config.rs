use crate::{
    config::{GenerateConfig, SinkConfig, SinkContext, SinkDescription},
    sinks::{
        util::{
            encoding::{EncodingConfig, StandardEncodings},
            BatchConfig, Compression, TowerRequestConfig,
        },
        Healthcheck,
    },
    template::Template,
    tls::TlsOptions,
};
use serde::{Deserialize, Serialize};
use vector_core::{sink::VectorSink, transform::DataType};

use super::conn;

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
    pub encoding: EncodingConfig<StandardEncodings>,
    #[serde(default)]
    pub compression: Compression,
    #[serde(default)]
    pub batch: BatchConfig,
    #[serde(default)]
    pub request: TowerRequestConfig,
    pub tls: Option<TlsOptions>,
}

fn host_key() -> String {
    crate::config::log_schema().host_key().to_string()
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
            encoding: StandardEncodings::Text.into(),
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
        conn::build_sink(
            self.clone(),
            &self.request,
            &self.tls,
            cx.proxy(),
            self.batch,
            self.compression,
            cx.acker(),
            &self.endpoint,
            &self.token,
        )
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "splunk_hec_logs"
    }
}
