use vector_lib::codecs::encoding::{FramingConfig, JsonSerializerConfig, SerializerConfig};
use vector_lib::configurable::configurable_component;

use crate::{
    codecs::{EncodingConfig, EncodingConfigWithFraming},
    config::{AcknowledgementsConfig, DataType, GenerateConfig, Input, SinkConfig, SinkContext},
    http::{Auth, MaybeAuth},
    sinks::{
        http::config::{HttpMethod, HttpSinkConfig},
        util::{
            http::RequestConfig, BatchConfig, Compression, RealtimeSizeBasedDefaultBatchSettings,
            UriSerde,
        },
        Healthcheck, VectorSink,
    },
    tls::TlsConfig,
};

/// Configuration for the `openobserve` sink.
#[configurable_component(sink("openobserve", "Deliver log events to OpenObserve."))]
#[derive(Clone, Debug)]
pub struct OpenObserveConfig {
    /// The OpenObserve endpoint to send data to.
    #[configurable(metadata(docs::examples = "http://localhost:5080/api/default/default/_json"))]
    uri: UriSerde,

    /// The user and password to authenticate with OpenObserve endpoint.
    #[configurable(derived)]
    auth: Option<Auth>,

    #[configurable(derived)]
    #[serde(default)]
    request: RequestConfig,

    /// The compression algorithm to use.
    #[configurable(derived)]
    #[serde(default = "Compression::gzip_default")]
    compression: Compression,

    #[configurable(derived)]
    encoding: EncodingConfig,

    /// The batch settings for the sink.
    #[configurable(derived)]
    #[serde(default)]
    pub batch: BatchConfig<RealtimeSizeBasedDefaultBatchSettings>,

    /// Controls how acknowledgements are handled for this sink.
    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    acknowledgements: AcknowledgementsConfig,

    /// The TLS settings for the connection.
    ///
    /// Optional, constrains TLS settings for this sink.
    #[configurable(derived)]
    tls: Option<TlsConfig>,
}

impl GenerateConfig for OpenObserveConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"
            endpoint = "http://localhost:5080/api/default/default/_json"
            Auth = "user: test@example.com, password: your_ingestion_password"
            encoding.codec = "json"
        "#,
        )
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "openobserve")]
impl SinkConfig for OpenObserveConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let request = self.request.clone();

        // OpenObserve supports native HTTP ingest endpoint. This configuration wraps
        // the vector HTTP sink to provide official support for OpenObserve. This sink will 
        // allow maintaining the vector OpenObserve sink independent of the vector HTTP sink
        // configuration and will allow to accomodate any future changes to the interface.
        let http_sink_config = HttpSinkConfig {
            uri: self.uri.clone(),
            compression: self.compression,
            auth: self.auth.choose_one(&self.uri.auth)?,
            method: HttpMethod::Post,
            tls: self.tls.clone(),
            request,
            acknowledgements: self.acknowledgements,
            batch: self.batch,
            headers: None,
            encoding: EncodingConfigWithFraming::new(
                Some(FramingConfig::Bytes),
                SerializerConfig::Json(JsonSerializerConfig::default()),
                self.encoding.transformer(),
            ),
            payload_prefix: "".into(), // Always newline delimited JSON
            payload_suffix: "".into(), // Always newline delimited JSON
        };

        http_sink_config.build(cx).await
    }

    fn input(&self) -> Input {
        Input::new(self.encoding.config().input_type() & DataType::Log)
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<super::OpenObserveConfig>();
    }
}
