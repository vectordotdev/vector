use std::{collections::VecDeque, num::NonZeroUsize};

use bytes::Bytes;
use tokio_tungstenite::tungstenite::{handshake::server::Request, Message};
use url::Url;
use uuid::Uuid;
use vector_config::configurable_component;
use vector_lib::{
    event::{Event, MaybeAsLogMut},
    lookup::lookup_v2::ConfigValuePath,
};

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

    const fn with_replay_from(replay_from: Uuid) -> Self {
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
    /// Returns configured size of the buffer.
    fn buffer_capacity(&self) -> usize;
    /// Extracts buffer replay request from the given connection request, based on configuration.
    fn extract_message_replay_request(&self, request: &Request) -> BufferReplayRequest;
    /// Adds a message ID that can be used for requesting replay into the event.
    /// Created ID is returned to be stored in the buffer.
    fn add_replay_message_id_to_event(&self, event: &mut Event) -> Uuid;
}

impl WsMessageBufferConfig for Option<MessageBufferingConfig> {
    fn should_buffer(&self) -> bool {
        self.is_some()
    }

    fn buffer_capacity(&self) -> usize {
        self.as_ref().map_or(0, |mb| mb.max_events.get())
    }

    fn extract_message_replay_request(&self, request: &Request) -> BufferReplayRequest {
        // Early return if buffering is disabled
        if self.is_none() {
            return BufferReplayRequest::NO_REPLAY;
        }

        // Early return if query params are missing
        let Some(query_params) = request.uri().query() else {
            return BufferReplayRequest::NO_REPLAY;
        };

        // Early return if there is no query param for replay
        if !query_params.contains(LAST_RECEIVED_QUERY_PARAM_NAME) {
            return BufferReplayRequest::NO_REPLAY;
        }

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
}
