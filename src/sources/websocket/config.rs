use serde_with::serde_as;
use snafu::ResultExt;
use vector_lib::codecs::decoding::{DeserializerConfig, FramingConfig};

use crate::common::websocket::WebSocketCommonConfig;
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

/// Defines the different shapes the `pong_message` config can take.
#[derive(Clone, Debug)]
#[configurable_component]
#[serde(untagged)] // Allows this enum to match different structures
pub enum PongMessage {
    /// For simple, backward-compatible exact matching.
    /// e.g., pong_message: "pong"
    Simple(String),

    /// For advanced matching strategies.
    /// e.g., pong_message: { type: contains, value: "pong" }
    Advanced(PongValidation),
}

/// Defines the advanced validation strategies for a pong message.
#[derive(Clone, Debug)]
#[configurable_component]
#[serde(tag = "type", content = "value")]
pub enum PongValidation {
    /// The entire message must be an exact match.
    Exact(String),
    /// The message must contain the value as a substring.
    Contains(String),
}

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

    /// An optional message to send to the server upon connection.
    #[configurable]
    #[serde(default)]
    pub initial_message: Option<String>,

    /// An optional application-level ping message to send over the WebSocket connection.
    /// If not set, a standard WebSocket ping control frame will be sent instead.
    #[configurable]
    #[serde(default)]
    pub ping_message: Option<String>,

    /// The expected application-level pong message to listen for as a response to a custom `ping_message`.
    /// This is only used when `ping_message` is also configured. When a custom ping is sent,
    /// receiving this specific message confirms that the connection is still alive.
    #[configurable]
    #[serde(default)]
    pub pong_message: Option<PongMessage>,

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
            initial_message: None,
            ping_message: None,
            pong_message: None,
            log_namespace: None,
        }
    }
}

impl_generate_config_from_default!(WebSocketConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "websocket")]
impl SourceConfig for super::config::WebSocketConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<sources::Source> {
        let tls =
            MaybeTlsSettings::from_config(self.common.tls.as_ref(), false).context(ConnectSnafu)?;
        let connector =
            WebSocketConnector::new(self.common.uri.clone(), tls, self.common.auth.clone())?;

        let log_namespace = cx.log_namespace(self.log_namespace);
        let decoder = DecodingConfig::new(self.framing.clone(), self.decoding.clone(), log_namespace)
            .build()?;

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

#[cfg(test)]
mod test {
    use super::*;
    use vector_lib::schema::Definition;
    use vector_lib::{config::LogNamespace, lookup::OwnedTargetPath, schema};
    use vrl::owned_value_path;
    use vrl::value::kind::{Collection, Kind};

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<WebSocketConfig>();
    }

    #[test]
    fn output_schema_definition_vector_namespace() {
        let config = WebSocketConfig {
            log_namespace: Some(true),
            ..Default::default()
        };

        let definition = config
            .outputs(LogNamespace::Vector)
            .remove(0)
            .schema_definition(true);

        let expected_definition =
            Definition::new_with_default_metadata(Kind::bytes(), [LogNamespace::Vector])
                .with_meaning(OwnedTargetPath::event_root(), "message")
                .with_metadata_field(
                    &owned_value_path!("vector", "source_type"),
                    Kind::bytes(),
                    None,
                )
                .with_metadata_field(
                    &owned_value_path!("vector", "ingest_timestamp"),
                    Kind::timestamp(),
                    None,
                );

        assert_eq!(definition, Some(expected_definition));
    }

    #[test]
    fn output_schema_definition_legacy_namespace() {
        let config = WebSocketConfig::default();

        let definition = config
            .outputs(LogNamespace::Legacy)
            .remove(0)
            .schema_definition(true);

        let expected_definition = schema::Definition::new_with_default_metadata(
            Kind::object(Collection::empty()),
            [LogNamespace::Legacy],
        )
        .with_event_field(
            &owned_value_path!("message"),
            Kind::bytes(),
            Some("message"),
        )
        .with_event_field(&owned_value_path!("timestamp"), Kind::timestamp(), None)
        .with_event_field(&owned_value_path!("source_type"), Kind::bytes(), None);

        assert_eq!(definition, Some(expected_definition));
    }
}
