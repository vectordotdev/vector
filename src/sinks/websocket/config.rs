use snafu::ResultExt;
use tokio_tungstenite::tungstenite::http::Uri;
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

    pub encoding: EncodingConfig,

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

#[derive(Clone, Debug)]
pub struct ValidatedWebSocketSink {
    uri: Uri,
    transformer: Transformer,
}

#[async_trait::async_trait]
impl ValidatedSink for WebSocketSinkConfig {
    type Validated = ValidatedWebSocketSink;

    fn validate(&self) -> crate::Result<ValidatedWebSocketSink> {
        let uri = WebSocketConnector::parse_uri(&self.common.uri)?;
        let transformer = self.encoding.transformer();
        Ok(ValidatedWebSocketSink { uri, transformer })
    }

    async fn build(
        &self,
        validated: &ValidatedWebSocketSink,
        _cx: SinkContext,
    ) -> crate::Result<(VectorSink, Healthcheck)> {
        // TLS settings may read certificate files from disk, so the connector is
        // resolved at build time rather than during validation.
        let ValidatedWebSocketSink { uri, transformer } = validated.clone();
        let connector = self.build_connector(uri)?;
        let serializer = self.encoding.build()?;
        let ws_sink = WebSocketSink::new(self, connector.clone(), transformer, serializer)?;

        Ok((
            VectorSink::from_event_streamsink(ws_sink),
            Box::pin(async move { connector.healthcheck().await }),
        ))
    }
}

impl WebSocketSinkConfig {
    fn build_connector(&self, uri: Uri) -> Result<WebSocketConnector, WebSocketError> {
        let tls =
            MaybeTlsSettings::from_config(self.common.tls.as_ref(), false).context(ConnectSnafu)?;
        Ok(WebSocketConnector::from_validated(
            uri,
            tls,
            self.common.auth.clone(),
        ))
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

    #[test]
    fn validate_rejects_malformed_uri() {
        let config = WebSocketSinkConfig {
            common: WebSocketCommonConfig {
                uri: "not a valid uri".to_string(),
                ..Default::default()
            },
            encoding: JsonSerializerConfig::default().into(),
            acknowledgements: Default::default(),
        };

        assert!(
            config.validate().is_err(),
            "a malformed URI should fail validation"
        );
    }

    #[test]
    fn validate_rejects_uri_without_host() {
        let config = WebSocketSinkConfig {
            common: WebSocketCommonConfig {
                uri: "ws:///path".to_string(),
                ..Default::default()
            },
            encoding: JsonSerializerConfig::default().into(),
            acknowledgements: Default::default(),
        };

        assert!(
            config.validate().is_err(),
            "a URI without a host should fail validation"
        );
    }
}
