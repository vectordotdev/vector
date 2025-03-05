use std::net::SocketAddr;

use vector_lib::codecs::JsonSerializerConfig;
use vector_lib::configurable::configurable_component;

use crate::{
    codecs::EncodingConfig,
    common::http::server_auth::HttpServerAuthConfig,
    config::{AcknowledgementsConfig, Input, SinkConfig, SinkContext},
    sinks::{Healthcheck, VectorSink},
    tls::TlsEnableableConfig,
};

use super::buffering::MessageBufferingConfig;
use super::sink::WebSocketListenerSink;

/// Configuration for the `websocket_server` sink.
#[configurable_component(sink(
    "websocket_server",
    "Deliver observability event data to websocket clients."
))]
#[derive(Clone, Debug)]
pub struct WebSocketListenerSinkConfig {
    /// The socket address to listen for connections on.
    ///
    /// This value _must_ include a port.
    #[configurable(metadata(docs::examples = "0.0.0.0:80"))]
    #[configurable(metadata(docs::examples = "localhost:80"))]
    pub address: SocketAddr,

    #[configurable(derived)]
    pub tls: Option<TlsEnableableConfig>,

    #[configurable(derived)]
    pub encoding: EncodingConfig,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,

    #[configurable(derived)]
    pub message_buffering: Option<MessageBufferingConfig>,

    #[configurable(derived)]
    pub auth: Option<HttpServerAuthConfig>,
}

impl Default for WebSocketListenerSinkConfig {
    fn default() -> Self {
        Self {
            address: "0.0.0.0:8080".parse().unwrap(),
            encoding: JsonSerializerConfig::default().into(),
            tls: None,
            acknowledgements: Default::default(),
            message_buffering: None,
            auth: None,
        }
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "websocket_server")]
impl SinkConfig for WebSocketListenerSinkConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let ws_sink = WebSocketListenerSink::new(self.clone(), cx)?;

        Ok((
            VectorSink::from_event_streamsink(ws_sink),
            Box::pin(async move { Ok(()) }),
        ))
    }

    fn input(&self) -> Input {
        Input::new(self.encoding.config().input_type())
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

impl_generate_config_from_default!(WebSocketListenerSinkConfig);

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<WebSocketListenerSinkConfig>();
    }
}
