use http::Uri;
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
    #[serde(default = "default_endpoint")]
    #[configurable(metadata(docs::examples = "http://localhost:5080/api/default/default/_json"))]
    uri: UriSerde,

    /// Authentication configuration for HTTP requests.
    #[configurable(derived)]
    auth: Option<Auth>,

    /// Outbound HTTP request settings.
    #[configurable(derived)]
    #[serde(default)]
    request: RequestConfig,

    /// The compression algorithm to use.
    #[configurable(derived)]
    #[serde(default = "Compression::gzip_default")]
    compression: Compression,

    /// Encoding configuration.
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
            uri = "http://localhost:5080/api/default/default/_json"
            Auth = "user: test@example.com, password: your_ingestion_password"
            encoding.codec = "json"
        "#,
        )
        .unwrap()
    }
}

fn default_endpoint() -> UriSerde {
    UriSerde {
        uri: Uri::from_static("http://localhost:5080/api/default/default/_json"),
        auth: None,
    }
}

/// This sink wraps the Vector HTTP sink to provide official support for OpenObserve's
/// native HTTP ingest endpoint. By doing so, it maintains a distinct configuration for
/// the OpenObserve sink, separate from the Vector HTTP sink. This approach ensures
/// that future changes to OpenObserve's interface can be accommodated without impacting
/// the underlying Vector HTTP sink.
#[async_trait::async_trait]
#[typetag::serde(name = "openobserve")]
impl SinkConfig for OpenObserveConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let request = self.request.clone();
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
