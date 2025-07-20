use vector_lib::codecs::decoding::{DeserializerConfig, FramingConfig};
use serde_with::serde_as;
use snafu::ResultExt;

use crate::{
    codecs::DecodingConfig,
    common::websocket::{ConnectSnafu, WebSocketConnector},
    config::{SourceConfig, SourceContext},
    serde::{default_decoding, default_framing_message_based},
    sources,
    tls::MaybeTlsSettings,
};
use vector_config::configurable_component;
use vector_lib::config::{LogNamespace, SourceOutput};
use crate::common::websocket::WebSocketCommonConfig;

/// Configuration for the `websocket` source.
#[serde_as]
#[configurable_component(source("websocket", "Collect events from a websocket endpoint.",))]
#[derive(Clone, Debug)]
pub struct WebSocketConfig {
    #[serde(flatten)]
    pub common: WebSocketCommonConfig,

    /// Decoder to use on each received message.
    #[configurable(derived)]
    #[serde(default = "default_decoding")]
    pub decoding: DeserializerConfig,

    /// Framing to use in the decoding.
    #[configurable(derived)]
    #[serde(default = "default_framing_message_based")]
    pub framing: FramingConfig,

    /// The namespace to use for logs. This overrides the global setting.
    #[configurable(metadata(docs::hidden))]
    #[serde(default)]
    pub log_namespace: Option<bool>,
}

impl Default for WebSocketConfig {
    fn default() -> Self {
        Self {
            common: WebSocketCommonConfig::default(),
            decoding: default_decoding(),
            framing: default_framing_message_based(),
            log_namespace: None,
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
        let tls = MaybeTlsSettings::from_config(self.common.tls.as_ref(), false).context(ConnectSnafu)?;
        let connector = WebSocketConnector::new(self.common.uri.clone(), tls, self.common.auth.clone())?;

        let log_namespace = cx.log_namespace(self.log_namespace);
        let decoder = self.get_decoding_config(Some(log_namespace)).build()?;

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

        vec![SourceOutput::new_maybe_logs(
            self.decoding.output_type(),
            schema_definition,
        )]
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}
