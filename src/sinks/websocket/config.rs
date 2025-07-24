use snafu::ResultExt;
use vector_lib::codecs::JsonSerializerConfig;
use vector_lib::configurable::configurable_component;

use crate::common::websocket::WebSocketCommonConfig;
use crate::{
    codecs::EncodingConfig,
    common::websocket::{ConnectSnafu, WebSocketConnector, WebSocketError},
    config::{AcknowledgementsConfig, GenerateConfig, Input, SinkConfig, SinkContext},
    sinks::{websocket::sink::WebSocketSink, Healthcheck, VectorSink},
    tls::MaybeTlsSettings,
};

/// Configuration for the `websocket` sink.
#[configurable_component(sink(
    "websocket",
    "Deliver observability event data to a websocket listener."
))]
#[derive(Clone, Debug)]
pub struct WebSocketSinkConfig {
    #[serde(flatten)]
    pub common: WebSocketCommonConfig,

    #[configurable(derived)]
    pub encoding: EncodingConfig,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
}

impl GenerateConfig for WebSocketSinkConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            common: WebSocketCommonConfig {
                ..Default::default()
            },
            encoding: JsonSerializerConfig::default().into(),
            acknowledgements: Default::default(),
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "websocket")]
impl SinkConfig for WebSocketSinkConfig {
    async fn build(&self, _cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let connector = self.build_connector()?;
        let ws_sink = WebSocketSink::new(self, connector.clone())?;

        Ok((
            VectorSink::from_event_streamsink(ws_sink),
            Box::pin(async move { connector.healthcheck().await }),
        ))
    }

    fn input(&self) -> Input {
        Input::new(self.encoding.config().input_type())
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

impl WebSocketSinkConfig {
    fn build_connector(&self) -> Result<WebSocketConnector, WebSocketError> {
        let tls =
            MaybeTlsSettings::from_config(self.common.tls.as_ref(), false).context(ConnectSnafu)?;
        WebSocketConnector::new(self.common.uri.clone(), tls, self.common.auth.clone())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<WebSocketSinkConfig>();
    }
}
