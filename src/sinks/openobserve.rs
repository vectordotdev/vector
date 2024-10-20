use vector_lib::configurable::configurable_component;

use crate::{
    codecs::{EncodingConfigWithFraming, Transformer},
    config::{AcknowledgementsConfig, GenerateConfig, Input, SinkConfig, SinkContext},
    http::Auth as HttpAuthConfig,
    sinks::{
        http::config::{HttpMethod, HttpSinkConfig},
        util::{
            http::RequestConfig, BatchConfig, Compression, RealtimeSizeBasedDefaultBatchSettings,
        },
        Healthcheck, VectorSink,
    },
    tls::TlsConfig,
};

/// Configuration for the `openobserve` sink.
#[configurable_component(sink("openobserve", "Deliver log events to OpenObserve."))]
#[derive(Clone, Debug)]
pub struct OpenObserveConfig {
    /// Wrap the HTTP sink configuration.
    pub http: HttpSinkConfig,
}

#[async_trait::async_trait]
#[typetag::serde(name = "openobserve")]
impl SinkConfig for OpenObserveConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let http = self.http.clone();
        let sink = HttpSink::new(http, cx)?;
        Ok((sink, sink.healthcheck()))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "openobserve"
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<super::OpenObserveConfig>();
    }
}

#[cfg(feature = "openobserve-integration-tests")]
#[cfg(test)]
mod integration_tests {}
