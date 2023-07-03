use std::num::NonZeroU64;

use codecs::decoding::{DeserializerConfig, FramingConfig};
use serde_with::serde_as;
use snafu::ResultExt;

use crate::{
    codecs::DecodingConfig,
    common::websocket::{ConnectSnafu, WebSocketConnector},
    config::{SourceConfig, SourceContext},
    http::Auth,
    serde::{default_decoding, default_framing_message_based},
    sources,
    tls::{MaybeTlsSettings, TlsEnableableConfig},
};
use vector_config::configurable_component;
use vector_core::config::{LogNamespace, SourceOutput};

/// Configuration for the `websocket` source.
#[serde_as]
#[configurable_component(source("websocket", "Pull logs from a websocket endpoint.",))]
#[derive(Clone, Debug)]
pub struct WebSocketConfig {
    /// The websocket endpoint
    ///
    /// The full path must be specified
    #[configurable(metadata(docs::examples = "wss://127.0.0.1:9898/logs"))]
    pub uri: String,

    /// Decoder to use on each received message.
    #[configurable(derived)]
    #[serde(default = "default_decoding")]
    pub decoding: DeserializerConfig,

    /// Framing to use in the decoding.
    #[configurable(derived)]
    #[serde(default = "default_framing_message_based")]
    pub framing: FramingConfig,

    /// TLS configuration.
    #[configurable(derived)]
    pub tls: Option<TlsEnableableConfig>,

    /// HTTP Authentication.
    #[configurable(derived)]
    pub auth: Option<Auth>,

    /// The namespace to use for logs. This overrides the global setting.
    #[configurable(metadata(docs::hidden))]
    #[serde(default)]
    pub log_namespace: Option<bool>,

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
}

impl Default for WebSocketConfig {
    fn default() -> Self {
        Self {
            uri: "ws://127.0.0.1:9898/logs".to_owned(),
            decoding: default_decoding(),
            framing: default_framing_message_based(),
            tls: None,
            auth: None,
            log_namespace: None,
            ping_interval: None,
            ping_timeout: None,
        }
    }
}

impl_generate_config_from_default!(WebSocketConfig);

impl WebSocketConfig {
    pub fn get_decoding_config(&self, log_namespace: Option<LogNamespace>) -> DecodingConfig {
        let decoding = self.decoding.clone();
        let framing = self.framing.clone();
        let log_namespace =
            log_namespace.unwrap_or_else(|| self.log_namespace.unwrap_or(false).into());

        DecodingConfig::new(framing, decoding, log_namespace)
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "websocket")]
impl SourceConfig for super::config::WebSocketConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<sources::Source> {
        let tls = MaybeTlsSettings::from_config(&self.tls, false).context(ConnectSnafu)?;
        let connector = WebSocketConnector::new(self.uri.clone(), tls, self.auth.clone())?;

        let log_namespace = cx.log_namespace(self.log_namespace);
        let decoder = self.get_decoding_config(Some(log_namespace)).build();

        Ok(Box::pin(super::source::recv_from_websocket(
            cx,
            self.clone(),
            super::source::WebSocketSourceParams {
                connector,
                decoder,
                log_namespace,
            },
        )))
    }

    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        let log_namespace = global_log_namespace.merge(self.log_namespace);

        let schema_definition = self
            .decoding
            .schema_definition(log_namespace)
            .with_standard_vector_source_metadata();

        vec![SourceOutput::new_logs(
            self.decoding.output_type(),
            schema_definition,
        )]
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}
