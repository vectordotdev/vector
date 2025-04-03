use crate::serde::default_decoding;
use std::{collections::VecDeque, net::SocketAddr, num::NonZeroUsize};

use bytes::Bytes;
use derivative::Derivative;
use tokio_tungstenite::tungstenite::{handshake::server::Request, Message};
use url::Url;
use uuid::Uuid;
use vector_config::configurable_component;
use vector_lib::{
    codecs::decoding::{format::Deserializer as _, DeserializerConfig},
    event::{Event, MaybeAsLogMut},
    lookup::lookup_v2::ConfigValuePath,
};
use vrl::prelude::VrlValueConvert;

/// Configuration for message buffering which enables message replay for clients that connect later.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct MessageBufferingConfig {
    /// Max events to hold in buffer.
    ///
    /// The buffer is backed by a ring buffer, so the oldest messages will be lost when the size
    /// limit is reached.
    #[serde(default = "default_max_events")]
    pub max_events: NonZeroUsize,

    /// Message ID path.
    ///
    /// This has to be defined to expose message ID to clients in the messages. Using that ID,
    /// clients can request replay starting from the message ID of their choosing.
    #[serde(default, skip_serializing_if = "crate::serde::is_default")]
    pub message_id_path: Option<ConfigValuePath>,

    #[configurable(derived)]
    pub client_ack_config: Option<BufferingAckConfig>,
}

/// Configuration for ACK support for message buffering.
/// Enabling ACK support makes it possible to replay messages for clients without requiring query
/// parameters at connection time. It moves the burden of tracking latest received messages from
/// clients to this component. It requires clients to respond to received messages with an ACK.
#[configurable_component]
#[derive(Clone, Debug, Derivative)]
pub struct BufferingAckConfig {
    #[configurable(derived)]
    #[derivative(Default(value = "default_decoding()"))]
    #[serde(default = "default_decoding")]
    pub ack_decoding: DeserializerConfig,

    /// Name of the field that contains the ACKed message ID. Use "." if message ID is the root of
    /// the message.
    pub message_id_path: ConfigValuePath,

    #[configurable(derived)]
    #[serde(default = "default_client_key_config")]
    pub client_key: ClientKeyConfig,
}

/// Configuration for client key used for tracking ACKed message for message buffering.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
#[configurable(metadata(
    docs::enum_tag_description = "The type of client key to use, when tracking ACKed message for message buffering."
))]
pub enum ClientKeyConfig {
    /// Use client IP address as the unique key for that client
    IpAddress {
        /// Set to true if port should be included with the IP address
        ///
        /// By default port is not included
        #[serde(default = "crate::serde::default_false")]
        with_port: bool,
    },
    /// Use the value of a header on connection request as the unique key for that client
    Header {
        /// Name of the header to use as value
        name: String,
    },
}

const fn default_client_key_config() -> ClientKeyConfig {
    ClientKeyConfig::IpAddress { with_port: false }
}

const fn default_max_events() -> NonZeroUsize {
    unsafe { NonZeroUsize::new_unchecked(1000) }
}

const LAST_RECEIVED_QUERY_PARAM_NAME: &str = "last_received";

pub struct BufferReplayRequest {
    should_replay: bool,
    replay_from: Option<Uuid>,
}

impl BufferReplayRequest {
    pub const NO_REPLAY: Self = Self {
        should_replay: false,
        replay_from: None,
    };
    pub const REPLAY_ALL: Self = Self {
        should_replay: true,
        replay_from: None,
    };

    pub const fn with_replay_from(replay_from: Uuid) -> Self {
        Self {
            should_replay: true,
            replay_from: Some(replay_from),
        }
    }

    pub fn replay_messages(
        &self,
        buffer: &VecDeque<(Uuid, Message)>,
        replay: impl FnMut(&(Uuid, Message)),
    ) {
        if self.should_replay {
            buffer
                .iter()
                .filter(|(id, _)| Some(*id) > self.replay_from)
                .for_each(replay);
        }
    }
}

pub trait WsMessageBufferConfig {
    /// Returns true if this configuration enables buffering.
    fn should_buffer(&self) -> bool;
    /// Generates key for a client based on connection request and address.
    /// This key should be used for storing client checkpoints (last ACKed message).
    fn client_key(&self, request: &Request, client_address: &SocketAddr) -> Option<String>;
    /// Returns configured size of the buffer.
    fn buffer_capacity(&self) -> usize;
    /// Extracts buffer replay request from the given connection request, based on configuration.
    fn extract_message_replay_request(
        &self,
        request: &Request,
        client_checkpoint: Option<Uuid>,
    ) -> BufferReplayRequest;
    /// Adds a message ID that can be used for requesting replay into the event.
    /// Created ID is returned to be stored in the buffer.
    fn add_replay_message_id_to_event(&self, event: &mut Event) -> Uuid;
    /// Handles ACK request and returns message ID, if available.
    fn handle_ack_request(&self, request: Message) -> Option<Uuid>;
}

impl WsMessageBufferConfig for Option<MessageBufferingConfig> {
    fn should_buffer(&self) -> bool {
        self.is_some()
    }

    fn client_key(&self, request: &Request, client_address: &SocketAddr) -> Option<String> {
        self.as_ref()
            .and_then(|mb| mb.client_ack_config.as_ref())
            .and_then(|ack| match &ack.client_key {
                ClientKeyConfig::IpAddress { with_port } => Some(if *with_port {
                    client_address.to_string()
                } else {
                    client_address.ip().to_string()
                }),
                ClientKeyConfig::Header { name } => request
                    .headers()
                    .get(name)
                    .and_then(|h| h.to_str().ok())
                    .map(ToString::to_string),
            })
    }

    fn buffer_capacity(&self) -> usize {
        self.as_ref().map_or(0, |mb| mb.max_events.get())
    }

    fn extract_message_replay_request(
        &self,
        request: &Request,
        client_checkpoint: Option<Uuid>,
    ) -> BufferReplayRequest {
        // Early return if buffering is disabled
        if self.is_none() {
            return BufferReplayRequest::NO_REPLAY;
        }

        let default_request = client_checkpoint
            .map(BufferReplayRequest::with_replay_from)
            // If we don't have ACK support, or don't have an ACK stored for the client,
            // default to no replay
            .unwrap_or(BufferReplayRequest::NO_REPLAY);

        // Early return if query params are missing
        let Some(query_params) = request.uri().query() else {
            return default_request;
        };

        // Early return if there is no query param for replay
        if !query_params.contains(LAST_RECEIVED_QUERY_PARAM_NAME) {
            return default_request;
        }

        // Even if we have an ACK stored, query param should override the cached state
        let base_url = Url::parse("ws://localhost").ok();
        match Url::options()
            .base_url(base_url.as_ref())
            .parse(request.uri().to_string().as_str())
        {
            Ok(url) => {
                if let Some((_, last_received_param_value)) = url
                    .query_pairs()
                    .find(|(k, _)| k == LAST_RECEIVED_QUERY_PARAM_NAME)
                {
                    match Uuid::parse_str(&last_received_param_value) {
                        Ok(last_received_val) => {
                            return BufferReplayRequest::with_replay_from(last_received_val)
                        }
                        Err(err) => {
                            warn!(message = "Parsing last received message UUID failed.", %err)
                        }
                    }
                }
            }
            Err(err) => {
                warn!(message = "Parsing request URL for websocket connection request failed.", %err)
            }
        }

        // Even if we can't find the provided message ID, we should dump whatever we have
        // buffered so far, because the provided message ID might have expired by now
        BufferReplayRequest::REPLAY_ALL
    }

    fn add_replay_message_id_to_event(&self, event: &mut Event) -> Uuid {
        let message_id = Uuid::now_v7();
        if let Some(MessageBufferingConfig {
            message_id_path: Some(ref message_id_path),
            ..
        }) = self
        {
            if let Some(log) = event.maybe_as_log_mut() {
                let mut buffer = [0; 36];
                let uuid = message_id.hyphenated().encode_lower(&mut buffer);
                log.value_mut()
                    .insert(message_id_path, Bytes::copy_from_slice(uuid.as_bytes()));
            }
        }
        message_id
    }

    fn handle_ack_request(&self, request: Message) -> Option<Uuid> {
        let ack_config = self.as_ref().and_then(|mb| mb.client_ack_config.as_ref())?;

        let parsed_message = ack_config
            .ack_decoding
            .build()
            .expect("Invalid `ack_decoding` config.")
            .parse(request.into_data().into(), Default::default())
            .inspect_err(|err| {
                debug!(message = "Parsing ACK request failed.", %err);
            })
            .ok()?;

        let Some(message_id_field) = parsed_message
            .first()?
            .maybe_as_log()?
            .value()
            .get(&ack_config.message_id_path)
        else {
            debug!("Couldn't find message ID in ACK request.");
            return None;
        };

        message_id_field
            .try_bytes_utf8_lossy()
            .map_err(|_| "Message ID is not a valid string.")
            .and_then(|id| {
                Uuid::parse_str(id.trim()).map_err(|_| "Message ID is not a valid UUID.")
            })
            .inspect_err(|err| debug!(message = "Parsing message ID in ACK request failed.", %err))
            .ok()
    }
}
