use futures_util::FutureExt;
use snafu::ResultExt;
use tower::ServiceBuilder;
use vector_core::{sink::VectorSink, stream::BatcherSettings, transform::DataType};

use crate::{config::{GenerateConfig, SinkConfig, SinkContext}, http::HttpClient, sinks::{Healthcheck, UriParseError, splunk_hec::{
            common::{build_healthcheck, build_uri, create_client, host_key},
            logs_new::service::Encoding,
        }, util::{BatchConfig, BatchSettings, Buffer, Compression, ServiceBuilderExt, TowerRequestConfig, encoding::{EncodingConfig, EncodingConfiguration, StandardEncodings}}}, template::Template, tls::TlsOptions};

use serde::{Deserialize, Serialize};

use super::{
    encoder::HecLogsEncoder,
    service::{HecLogsRequestBuilder, HecLogsRetry, HecLogsService, HttpRequestBuilder},
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
    pub encoding: EncodingConfig<Encoding>,
    #[serde(default)]
    pub compression: Compression,
    #[serde(default)]
    pub batch: BatchConfig,
    #[serde(default)]
    pub request: TowerRequestConfig,
    pub tls: Option<TlsOptions>,
    // pub encoding_standard: EncodingConfig<StandardEncodings>,
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
            encoding: Encoding::Text.into(),
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
        // Build the service that will make requests
        let content_encoding = self.compression.content_encoding();
        let request_settings = self.request.unwrap_with(&TowerRequestConfig::default());
        let http_request_builder = HttpRequestBuilder {
            endpoint: self.endpoint.clone(),
            token: self.token.clone(),
            content_encoding: content_encoding,
        };
        let service = ServiceBuilder::new()
            .settings(request_settings, HecLogsRetry)
            .service(HecLogsService::new(client.clone(), http_request_builder));

        // Build the encoder that will be used to turn Vec<Event> into Vec<u8>
        let encoding = HecLogsEncoder {
            encoding: self.encoding.codec().clone(),
        };

        // Build the request builder that will be used to build Requests out of encoded Events
        let request_builder = HecLogsRequestBuilder {
            encoding: encoding.into(),
            compression: self.compression.clone(),
            endpoint: self.endpoint.clone(),
            indexed_fields: self.indexed_fields.clone(),
            index: self.index.clone(),
            sourcetype: self.sourcetype.clone(),
            source: self.source.clone(),
            host_key: self.host_key.clone(),
            // encoding_standard: self.encoding_standard.clone(),
        };
        

        let batch_settings = BatchSettings::<Buffer>::default()
            .bytes(1_000_000)
            .timeout(1)
            .parse_config(self.batch)?
            .into_batcher_settings()?;

        // Build the sink with a request builder, service, context
        let sink = HecLogsSink {
            service,
            request_builder,
            context: cx,
            batch_settings: batch_settings,
            source: self.source.clone(),
        };

        Ok(VectorSink::Stream(Box::new(sink)))
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
