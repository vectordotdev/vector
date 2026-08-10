use snafu::ResultExt;
use vector_lib::{codecs::JsonSerializerConfig, configurable::configurable_component};

use crate::{
    codecs::{EncodingConfig, Transformer},
    common::websocket::{ConnectSnafu, WebSocketCommonConfig, WebSocketConnector, WebSocketError},
    config::{
        AcknowledgementsConfig, GenerateConfig, Input, SinkConfig, SinkContext, ValidatedSink,
    },
    sinks::{Healthcheck, VectorSink, websocket::sink::WebSocketSink},
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
    fn generate_config() -> serde_json::Value {
        serde_json::to_value(Self {
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
    fn input(&self) -> Input {
        Input::new(self.encoding.config().input_type())
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

/// Purely validated `websocket` sink configuration.
///
/// Holds the built encoding components so `build` does not redo the (pure)
/// structural validation. The connector is NOT retained here: it resolves TLS
/// settings (which may read certificate files from disk), so it is built in
/// `build` instead.
#[derive(Clone, Debug)]
pub struct ValidatedWebSocketSink {
    transformer: Transformer,
}

#[async_trait::async_trait]
impl ValidatedSink for WebSocketSinkConfig {
    type Validated = ValidatedWebSocketSink;

    fn validate(&self) -> crate::Result<ValidatedWebSocketSink> {
        let transformer = self.encoding.transformer();
        Ok(ValidatedWebSocketSink { transformer })
    }

    async fn build(
        &self,
        validated: &ValidatedWebSocketSink,
        _cx: SinkContext,
    ) -> crate::Result<(VectorSink, Healthcheck)> {
        // TLS settings may read certificate files from disk, so the connector is
        // resolved at build time rather than during pure validation.
        let connector = self.build_connector()?;
        let ValidatedWebSocketSink { transformer } = validated;
        let serializer = self.encoding.build()?;
        let ws_sink = WebSocketSink::new(self, connector.clone(), transformer.clone(), serializer)?;

        Ok((
            VectorSink::from_event_streamsink(ws_sink),
            Box::pin(async move { connector.healthcheck().await }),
        ))
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
    use vector_lib::codecs::encoding::SerializerConfig;

    use crate::config::ValidatedSink;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<WebSocketSinkConfig>();
    }

    #[test]
    fn validate_produces_usable_state() {
        let config = WebSocketSinkConfig {
            common: WebSocketCommonConfig {
                uri: "ws://127.0.0.1:8080".to_string(),
                ..Default::default()
            },
            encoding: JsonSerializerConfig::default().into(),
            acknowledgements: Default::default(),
        };
        let _validated = config.validate().expect("validation should succeed");
        // Serializer construction is deferred to `build`; validation retains the
        // transformer so `build` can construct the serializer.
        assert!(matches!(
            config.encoding.config(),
            SerializerConfig::Json(_)
        ));
    }
}
