use std::{path::PathBuf, time::Duration};

use futures::TryFutureExt;
use serde_with::serde_as;
use snafu::ResultExt;
use vector_config::configurable_component;
use vector_lib::{
    codecs::{
        EncodingConfig,
        decoding::{DeserializerConfig, FramingConfig},
    },
    config::{LogNamespace, SourceOutput},
    lookup::lookup_v2::ConfigValuePath,
};

use super::source::{WebSocketSource, WebSocketSourceParams};
use crate::{
    codecs::DecodingConfig,
    common::websocket::{ConnectSnafu, WebSocketCommonConfig, WebSocketConnector},
    config::{SourceConfig, SourceContext},
    serde::{default_decoding, default_framing_message_based},
    sources::Source,
    tls::MaybeTlsSettings,
};

/// Defines the different shapes the `pong_message` config can take.
#[derive(Clone, Debug)]
#[configurable_component]
#[serde(untagged)]
pub enum PongMessage {
    /// For exact string matching.
    /// e.g., `pong_message`: "pong"
    Simple(String),

    /// For advanced matching strategies.
    /// e.g., `pong_message`: { type: contains, value: "pong" }
    Advanced(PongValidation),
}

impl PongMessage {
    #[must_use]
    pub fn matches(&self, msg: &str) -> bool {
        match self {
            PongMessage::Simple(expected) => msg == expected,
            PongMessage::Advanced(validation) => validation.matches(msg),
        }
    }
}

/// Defines the advanced validation strategies for a pong message.
#[derive(Clone, Debug)]
#[configurable_component]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
#[configurable(metadata(
    docs::enum_tag_description = "The matching strategy to use for the pong message."
))]
pub enum PongValidation {
    /// The entire message must be an exact match.
    Exact {
        /// The string value to match against.
        value: String,
    },

    /// The message must contain the value as a substring.
    Contains {
        /// The string value to match against.
        value: String,
    },
}

impl PongValidation {
    #[must_use]
    pub fn matches(&self, msg: &str) -> bool {
        match self {
            PongValidation::Exact { value: expected } => msg == expected,
            PongValidation::Contains { value: substring } => msg.contains(substring),
        }
    }
}

/// Configuration for ACK support for message replay.
///
/// Enabling ACK support removes the need for client to keep track of latest received message.
/// It moves the burden of tracking latest received messages from this component to the server.
/// This will only work if the server has it enabled. This will make the client respond to received
/// messages with an ACK.
#[configurable_component]
#[derive(Clone, Debug, Derivative)]
pub struct ReplayAckConfig {
    #[configurable(derived)]
    pub encoding: EncodingConfig,

    /// Name of the field that should contain the ACKed message ID.
    ///
    /// Use "." if message ID is the root of the message.
    pub message_id_path: ConfigValuePath,
}

/// Configuration for message replay which allows replaying missed messages from servers that
/// support message buffering.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct MessageReplayConfig {
    /// The directory used to persist latest received message ID.
    ///
    /// By default, the [global `data_dir` option][global_data_dir] is used.
    /// Make sure the running user has write permissions to this directory.
    ///
    /// If this directory is specified, then Vector will attempt to create it.
    ///
    /// This is only in effect if [`Self::stateful`] is enabled.
    ///
    /// [global_data_dir]: https://vector.dev/docs/reference/configuration/global-options/#data_dir
    #[serde(default)]
    #[configurable(metadata(docs::examples = "/var/local/lib/vector/"))]
    #[configurable(metadata(docs::human_name = "Data Directory"))]
    pub data_dir: Option<PathBuf>,

    /// Whether the latest received message ID should be kept across Vector restarts.
    #[serde(default)]
    pub stateful: bool,

    /// Message ID path.
    ///
    /// This has to be defined to read message ID from messages. Using that ID,
    /// replay can be requested at reconnection and ACK messages can be sent.
    pub message_id_path: ConfigValuePath,

    #[configurable(derived)]
    pub ack: Option<ReplayAckConfig>,
}

/// Configuration for the `websocket` source.
#[serde_as]
#[configurable_component(source("websocket", "Collect events from a websocket endpoint.",))]
#[derive(Clone, Debug)]
pub struct WebSocketConfig {
    #[serde(flatten)]
    pub common: WebSocketCommonConfig,

    /// Decoder to use on each received message.
    #[serde(default = "default_decoding")]
    pub decoding: DeserializerConfig,

    /// Framing to use in the decoding.
    #[serde(default = "default_framing_message_based")]
    pub framing: FramingConfig,

    /// Number of seconds before timing out while connecting.
    #[serde_as(as = "serde_with::DurationSeconds<u64>")]
    #[serde(default = "default_connect_timeout_secs")]
    #[configurable(metadata(docs::examples = 10))]
    pub connect_timeout_secs: Duration,

    /// Number of seconds before timing out while waiting for a reply to the initial message.
    /// This is only used when `initial_message` is also configured.
    #[serde_as(as = "serde_with::DurationSeconds<u64>")]
    #[serde(default = "default_initial_message_timeout_secs")]
    #[configurable(metadata(docs::examples = 5))]
    pub initial_message_timeout_secs: Duration,

    /// An optional message to send to the server upon connection.
    #[configurable(metadata(docs::examples = "SUBSCRIBE logs"))]
    #[serde(default)]
    pub initial_message: Option<String>,

    /// An optional application-level ping message to send over the WebSocket connection.
    /// If not set, a standard WebSocket ping control frame is sent instead.
    #[serde(default)]
    pub ping_message: Option<String>,

    /// The expected application-level pong message to listen for as a response to a custom `ping_message`.
    /// This is only used when `ping_message` is also configured. When a custom ping is sent,
    /// receiving this specific message confirms that the connection is still alive.
    #[serde(default)]
    pub pong_message: Option<PongMessage>,

    #[configurable(derived)]
    #[serde(default)]
    pub replay: Option<MessageReplayConfig>,

    /// The namespace to use for logs. This overrides the global setting.
    #[configurable(metadata(docs::hidden))]
    #[serde(default)]
    pub log_namespace: Option<bool>,
}

const fn default_connect_timeout_secs() -> Duration {
    Duration::from_secs(30)
}

const fn default_initial_message_timeout_secs() -> Duration {
    Duration::from_secs(2)
}

impl Default for WebSocketConfig {
    fn default() -> Self {
        Self {
            common: WebSocketCommonConfig::default(),
            decoding: default_decoding(),
            framing: default_framing_message_based(),
            connect_timeout_secs: default_connect_timeout_secs(),
            initial_message: None,
            initial_message_timeout_secs: default_initial_message_timeout_secs(),
            ping_message: None,
            pong_message: None,
            replay: None,
            log_namespace: None,
        }
    }
}

impl_generate_config_from_default!(WebSocketConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "websocket")]
impl SourceConfig for WebSocketConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<Source> {
        let tls =
            MaybeTlsSettings::from_config(self.common.tls.as_ref(), false).context(ConnectSnafu)?;
        let uri = WebSocketConnector::parse_uri(&self.common.uri)?;
        let connector = WebSocketConnector::from_validated(uri, tls, self.common.auth.clone());
        let log_namespace = cx.log_namespace(self.log_namespace);
        let decoder =
            DecodingConfig::new(self.framing.clone(), self.decoding.clone(), log_namespace)
                .build()?;

        let data_dir = if self.replay.as_ref().map(|r| r.stateful).unwrap_or(false) {
            // add the source name as a subdir, so that multiple sources can
            // operate within the same given data_dir (e.g. the global one)
            Some(cx.globals.resolve_and_make_data_subdir(
                self.replay.as_ref().and_then(|r| r.data_dir.as_ref()),
                cx.key.id(),
            )?)
        } else {
            None
        };

        let params = WebSocketSourceParams {
            connector,
            tls,
            decoder,
            log_namespace,
            data_dir,
        };

        let source = WebSocketSource::new(self.clone(), params)?;

        Ok(Box::pin(source.run(cx).map_err(Into::into)))
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
    use vector_lib::{config::LogNamespace, lookup::OwnedTargetPath, schema, schema::Definition};
    use vrl::{
        owned_value_path,
        value::kind::{Collection, Kind},
    };

    use super::*;

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
