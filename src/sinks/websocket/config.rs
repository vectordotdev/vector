use std::num::NonZeroU64;

use snafu::ResultExt;
use vector_lib::codecs::JsonSerializerConfig;
use vector_lib::configurable::configurable_component;

use crate::{
    codecs::EncodingConfig,
    config::{AcknowledgementsConfig, GenerateConfig, Input, SinkConfig, SinkContext},
    http::Auth,
    sinks::{
        websocket::sink::{ConnectSnafu, WebSocketConnector, WebSocketError, WebSocketSink},
        Healthcheck, VectorSink,
    },
    tls::{MaybeTlsSettings, TlsEnableableConfig},
};

/// Configuration for the `websocket` sink.
#[configurable_component(sink(
    "websocket",
    "Deliver observability event data to a websocket listener."
))]
#[derive(Clone, Debug)]
pub struct WebSocketSinkConfig {
    /// The WebSocket URI to connect to.
    ///
    /// This should include the protocol and host, but can also include the port, path, and any other valid part of a URI.
    pub uri: String,

    #[configurable(derived)]
    pub tls: Option<TlsEnableableConfig>,

    #[configurable(derived)]
    pub encoding: EncodingConfig,

    /// The interval, in seconds, between sending [Ping][ping]s to the remote peer.
    ///
    /// If this option is not configured, pings are not sent on an interval.
    ///
    /// If the `ping_timeout` is not set, pings are still sent but there is no expectation of pong
    /// response times.
    ///
    /// [ping]: https://www.rfc-editor.org/rfc/rfc6455#section-5.5.2
    #[configurable(metadata(docs::type_unit = "seconds"))]
    pub ping_interval: Option<NonZeroU64>,

    /// The number of seconds to wait for a [Pong][pong] response from the remote peer.
    ///
    /// If a response is not received within this time, the connection is re-established.
    ///
    /// [pong]: https://www.rfc-editor.org/rfc/rfc6455#section-5.5.3
    // NOTE: this option is not relevant if the `ping_interval` is not configured.
    #[configurable(metadata(docs::type_unit = "seconds"))]
    pub ping_timeout: Option<NonZeroU64>,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,

    #[configurable(derived)]
    pub auth: Option<Auth>,
}

impl GenerateConfig for WebSocketSinkConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            uri: "ws://127.0.0.1:9000/endpoint".into(),
            tls: None,
            encoding: JsonSerializerConfig::default().into(),
            ping_interval: None,
            ping_timeout: None,
            acknowledgements: Default::default(),
            auth: None,
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
        let tls = MaybeTlsSettings::from_config(&self.tls, false).context(ConnectSnafu)?;
        WebSocketConnector::new(self.uri.clone(), tls, self.auth.clone())
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
