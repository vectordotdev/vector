use std::{borrow::Cow, fs, path::PathBuf, pin::Pin};

use bytes::BytesMut;
use chrono::{DateTime, Utc};
use futures::{Sink, Stream, StreamExt, pin_mut, sink::SinkExt};
use snafu::Snafu;
use tokio::{sync::mpsc::Sender, time};
use tokio_tungstenite::tungstenite::{
    Message, error::Error as TungsteniteError, protocol::CloseFrame,
};
use tokio_util::codec::Encoder;
use vector_lib::{
    EstimatedJsonEncodedSizeOf,
    codecs::{DecoderFramedRead, encoding::Serializer},
    config::LogNamespace,
    event::{Event, EventMetadata, LogEvent},
    internal_event::{CountByteSize, EventsReceived, InternalEventHandle as _},
};
use vrl::{core::Value, prelude::VrlValueConvert};

use crate::{
    SourceSender,
    codecs::Decoder,
    common::websocket::{PingInterval, WebSocketConnector, is_closed},
    config::SourceContext,
    internal_events::{
        ConnectionOpen, OpenGauge, PROTOCOL, WebSocketBytesReceived,
        WebSocketConnectionFailedError, WebSocketConnectionShutdown, WebSocketKind,
        WebSocketMessageReceived, WebSocketReceiveError, WebSocketSendError,
    },
    sources::websocket::config::WebSocketConfig,
    vector_lib::codecs::StreamDecodingError,
};

macro_rules! fail_with_event {
    ($context:expr_2021) => {{
        emit!(WebSocketConnectionFailedError {
            error: Box::new($context.build())
        });
        return $context.fail();
    }};
}

type WebSocketSink = Pin<Box<dyn Sink<Message, Error = TungsteniteError> + Send>>;
type WebSocketStream = Pin<Box<dyn Stream<Item = Result<Message, TungsteniteError>> + Send>>;

const LAST_RECEIVED_MESSAGE_ID_FILE_NAME: &str = "last_received_message_id";
const LAST_RECEIVED_MESSAGE_ID_QUERY_PARAM_NAME: &str = "last_received";

pub(crate) struct WebSocketSourceParams {
    pub connector: WebSocketConnector,
    pub decoder: Decoder,
    pub log_namespace: LogNamespace,
    pub data_dir: Option<PathBuf>,
}

pub(crate) struct WebSocketSource {
    config: WebSocketConfig,
    params: WebSocketSourceParams,
    last_received_message_id: Option<String>,
    ack_serializer: Option<Serializer>,
}

#[derive(Debug, Snafu)]
pub enum WebSocketSourceError {
    #[snafu(display("Connection attempt timed out"))]
    ConnectTimeout,

    #[snafu(display("Server did not respond to the initial message in time"))]
    InitialMessageTimeout,

    #[snafu(display(
        "The connection was closed after sending the initial message, but before a response."
    ))]
    ConnectionClosedPrematurely,

    #[snafu(display("Connection closed by server with code '{}' and reason: '{}'", frame.code, frame.reason))]
    RemoteClosed { frame: CloseFrame<'static> },

    #[snafu(display("Connection closed by server without a close frame"))]
    RemoteClosedEmpty,

    #[snafu(display("Connection timed out while waiting for a pong response"))]
    PongTimeout,

    #[snafu(display("A WebSocket error occurred: {}", source))]
    Tungstenite { source: TungsteniteError },
}

impl WebSocketSource {
    pub fn new(config: WebSocketConfig, params: WebSocketSourceParams) -> crate::Result<Self> {
        let last_received_message_id = 'restore: {
            if let Some(path) = params
                .data_dir
                .as_ref()
                .map(|p| p.join(LAST_RECEIVED_MESSAGE_ID_FILE_NAME))
            {
                let message_id = match fs::read_to_string(&path) {
                    Ok(contents) => contents,
                    Err(err) => match err.kind() {
                        std::io::ErrorKind::NotFound => {
                            if let Some(parent) = path.parent()
                                && parent != ""
                                && !fs::metadata(parent).is_ok_and(|m| m.is_dir())
                            {
                                return Err(format!(
                                    "Message replay persistence path directory ({}) doesn't exist. This will prevent persisting last received message. Fix the `persistence_path` to ensure export works.",
                                    parent.to_str().unwrap_or(""),
                                )
                                .into());
                            }
                            break 'restore None;
                        }
                        _ => {
                            return Err(format!(
                                "Couldn't open \"{}\" to read latest message received. {}",
                                path.to_str().unwrap_or(""),
                                err
                            )
                            .into());
                        }
                    },
                };
                Some(message_id)
            } else {
                None
            }
        };
        let mut ack_serializer = None;
        if let Some(replay_config) = &config.replay
            && let Some(replay_ack_config) = &replay_config.ack
        {
            ack_serializer = Some(replay_ack_config.encoding.build()?);
        }
        Ok(Self {
            config,
            params,
            last_received_message_id,
            ack_serializer,
        })
    }

    pub async fn run(mut self, cx: SourceContext) -> Result<(), WebSocketSourceError> {
        let _open_token = OpenGauge::new().open(|count| emit!(ConnectionOpen { count }));
        let mut ping_manager = PingManager::new(&self.config);

        let mut out = cx.out;

        let (ack_tx, mut ack_rx) = tokio::sync::mpsc::channel(10);
        let (ws_sink, ws_source) = self.connect(ack_tx.clone(), &mut out).await?;

        pin_mut!(ws_sink, ws_source);

        let stateful = self.config.replay.as_ref().is_some_and(|r| r.stateful);

        loop {
            let result = tokio::select! {
                biased;

                _ = cx.shutdown.clone() => {
                    info!("Received shutdown signal.");
                    break;
                },

                res = ping_manager.tick(&mut ws_sink) => res,

                Some(ack) = ack_rx.recv() => {
                    self.last_received_message_id = Some(ack.clone());

                    if stateful {
                        self.store_last_message_id().await;
                    }

                    // Ignore failed acks
                    Ok(self.send_ack(&mut ws_sink, ack).await)
                }

                Some(msg_result) = ws_source.next() => {
                    match msg_result {
                        Ok(msg) => self.handle_message(msg, &mut ping_manager, ack_tx.clone(), &mut out).await,
                        Err(error) => {
                            emit!(WebSocketReceiveError { error: &error });
                            Err(WebSocketSourceError::Tungstenite { source: error })
                        }
                    }
                }

            };

            if let Err(error) = result {
                match error {
                    WebSocketSourceError::RemoteClosed { frame } => {
                        warn!(
                            message = "Connection closed by server.",
                            code = %frame.code,
                            reason = %frame.reason
                        );
                        emit!(WebSocketConnectionShutdown);
                    }
                    WebSocketSourceError::RemoteClosedEmpty => {
                        warn!("Connection closed by server without a close frame.");
                        emit!(WebSocketConnectionShutdown);
                    }
                    WebSocketSourceError::PongTimeout => {
                        error!("Disconnecting due to pong timeout.");
                        emit!(WebSocketReceiveError {
                            error: &TungsteniteError::Io(std::io::Error::new(
                                std::io::ErrorKind::TimedOut,
                                "Pong timeout"
                            ))
                        });
                        emit!(WebSocketConnectionShutdown);
                        return Err(error);
                    }
                    WebSocketSourceError::Tungstenite { source: ws_err } => {
                        if is_closed(&ws_err) {
                            emit!(WebSocketConnectionShutdown);
                        }
                        error!(message = "WebSocket connection error.", error = %ws_err);
                    }
                    // These errors should only happen during `connect` or `reconnect`,
                    // not in the main loop's result.
                    WebSocketSourceError::ConnectTimeout
                    | WebSocketSourceError::InitialMessageTimeout
                    | WebSocketSourceError::ConnectionClosedPrematurely => {
                        unreachable!(
                            "Encountered a connection-time error during runtime: {error:?}"
                        );
                    }
                }

                if self
                    .reconnect(ack_tx.clone(), &mut out, &mut ws_sink, &mut ws_source)
                    .await
                    .is_err()
                {
                    break;
                }
            }
        }

        Ok(())
    }

    async fn store_last_message_id(&self) {
        if let Some(path) = self
            .params
            .data_dir
            .as_ref()
            .map(|p| p.join(LAST_RECEIVED_MESSAGE_ID_FILE_NAME))
            && let Some(last_message_id) = &self.last_received_message_id
            && let Err(err) = tokio::fs::write(path, last_message_id).await
        {
            error!(message = "Persisting last received message id failed.", %err);
        }
    }

    async fn handle_message(
        &self,
        msg: Message,
        ping_manager: &mut PingManager,
        ack_tx: Sender<String>,
        out: &mut SourceSender,
    ) -> Result<(), WebSocketSourceError> {
        match msg {
            Message::Pong(_) => {
                ping_manager.record_pong();
                Ok(())
            }
            Message::Text(msg_txt) => {
                if self.is_custom_pong(&msg_txt) {
                    ping_manager.record_pong();
                    debug!("Received custom pong response.");
                } else {
                    self.process_message(&msg_txt, WebSocketKind::Text, ack_tx, out)
                        .await;
                }
                Ok(())
            }
            Message::Binary(msg_bytes) => {
                self.process_message(&msg_bytes, WebSocketKind::Binary, ack_tx, out)
                    .await;
                Ok(())
            }
            Message::Ping(_) => Ok(()),
            Message::Close(frame) => Self::handle_close_frame(frame),
            Message::Frame(_) => {
                warn!("Unsupported message type received: frame.");
                Ok(())
            }
        }
    }

    async fn process_message<T>(
        &self,
        payload: &T,
        kind: WebSocketKind,
        ack_tx: Sender<String>,
        out: &mut SourceSender,
    ) where
        T: AsRef<[u8]> + ?Sized,
    {
        let payload_bytes = payload.as_ref();

        emit!(WebSocketBytesReceived {
            byte_size: payload_bytes.len(),
            url: &self.config.common.uri,
            protocol: PROTOCOL,
            kind,
        });
        let mut stream = DecoderFramedRead::new(payload_bytes, self.params.decoder.clone());

        while let Some(result) = stream.next().await {
            match result {
                Ok((events, _)) => {
                    if events.is_empty() {
                        continue;
                    }

                    let event_count = events.len();
                    let byte_size = events.estimated_json_encoded_size_of();

                    register!(EventsReceived).emit(CountByteSize(event_count, byte_size));
                    emit!(WebSocketMessageReceived {
                        count: event_count,
                        byte_size,
                        url: &self.config.common.uri,
                        protocol: PROTOCOL,
                        kind,
                    });

                    let now = Utc::now();

                    // Extract message IDs to remember the last one and to send ACKs if needed
                    let message_ids = self.extract_message_ids(events.iter());

                    let events_with_meta = events.into_iter().map(|mut event| {
                        if let Event::Log(event) = &mut event {
                            self.add_metadata(event, now);
                        }
                        event
                    });

                    if let Err(error) = out.send_batch(events_with_meta).await {
                        error!(message = "Error sending events.", %error);
                    }

                    // We only care about the latest message ID, because it is used to replay
                    if let Some(last_message_id) = message_ids.last() {
                        _ = ack_tx.send(last_message_id.clone()).await;
                    }
                }
                Err(error) => {
                    if !error.can_continue() {
                        break;
                    }
                }
            }
        }
    }

    fn extract_message_ids<'a>(&self, events: impl Iterator<Item = &'a Event>) -> Vec<String> {
        let mut message_ids = Vec::new();
        if let Some(replay_config) = &self.config.replay {
            message_ids.extend(events
                .filter_map(|event| {
                    if let Event::Log(event) = event {
                        event
                            .get_message()
                            .unwrap_or(event.value())
                            .get(&replay_config.message_id_path)
                            .and_then(|v| {
                                v.try_bytes_utf8_lossy()
                                    .inspect_err(|err| debug!(message = "Message ID is not a valid string.", %err))
                                    .ok()
                            })
                            .map(Cow::into_owned)
                    } else {
                        None
                    }
                }));
        }
        message_ids
    }

    fn add_metadata(&self, event: &mut LogEvent, now: DateTime<Utc>) {
        self.params
            .log_namespace
            .insert_standard_vector_source_metadata(event, WebSocketConfig::NAME, now);
    }

    async fn reconnect(
        &self,
        ack_tx: Sender<String>,
        out: &mut SourceSender,
        ws_sink: &mut WebSocketSink,
        ws_source: &mut WebSocketStream,
    ) -> Result<(), WebSocketSourceError> {
        info!("Reconnecting to WebSocket...");

        let (new_sink, new_source) = self.connect(ack_tx, out).await?;

        *ws_sink = new_sink;
        *ws_source = new_source;

        info!("Reconnected to Websocket.");

        Ok(())
    }

    async fn connect(
        &self,
        ack_tx: Sender<String>,
        out: &mut SourceSender,
    ) -> Result<(WebSocketSink, WebSocketStream), WebSocketSourceError> {
        let (mut ws_sink, mut ws_source) = self.try_create_sink_and_stream().await?;

        if self.config.initial_message.is_some() {
            self.send_initial_message(&mut ws_sink, &mut ws_source, ack_tx, out)
                .await?;
        }

        Ok((ws_sink, ws_source))
    }

    async fn try_create_sink_and_stream(
        &self,
    ) -> Result<(WebSocketSink, WebSocketStream), WebSocketSourceError> {
        // Create a new connector with `last_received` query param in the URL
        // If needed
        let last_received_message_connector =
            self.last_received_message_id
                .as_ref()
                .and_then(|last_received_message_id| {
                    let mut new_uri = url::Url::parse(&self.config.common.uri).ok()?;
                    new_uri.query_pairs_mut().append_pair(
                        LAST_RECEIVED_MESSAGE_ID_QUERY_PARAM_NAME,
                        last_received_message_id,
                    );
                    let new_uri = WebSocketConnector::parse_uri(new_uri.as_str())
                        .inspect_err(|err| {
                            warn!(message = "Sending last_received message ID to the server failed.", %err);
                        })
                        .ok()?;
                    Some(self.params.connector.clone().with_new_uri(new_uri))
                });
        let connector = if self.config.replay.is_some() {
            last_received_message_connector
                .as_ref()
                .unwrap_or(&self.params.connector)
        } else {
            &self.params.connector
        };
        let ws_stream = connector
            .connect_backoff_with_timeout(self.config.connect_timeout_secs)
            .await;

        let (sink, stream) = ws_stream.split();

        Ok((Box::pin(sink), Box::pin(stream)))
    }

    async fn send_initial_message(
        &self,
        ws_sink: &mut WebSocketSink,
        ws_source: &mut WebSocketStream,
        ack_tx: Sender<String>,
        out: &mut SourceSender,
    ) -> Result<(), WebSocketSourceError> {
        let initial_message = self.config.initial_message.as_ref().unwrap();
        ws_sink
            .send(Message::Text(initial_message.clone()))
            .await
            .map_err(|error| {
                emit!(WebSocketSendError { error: &error });
                WebSocketSourceError::Tungstenite { source: error }
            })?;

        debug!("Sent initial message, awaiting response from server.");

        let response =
            match time::timeout(self.config.initial_message_timeout_secs, ws_source.next()).await {
                Ok(Some(msg)) => msg,
                Ok(None) => fail_with_event!(ConnectionClosedPrematurelySnafu),
                Err(_) => fail_with_event!(InitialMessageTimeoutSnafu),
            };

        let message = response.map_err(|source| {
            emit!(WebSocketReceiveError { error: &source });
            WebSocketSourceError::Tungstenite { source }
        })?;

        match message {
            Message::Text(txt) => {
                self.process_message(&txt, WebSocketKind::Text, ack_tx, out)
                    .await;
                Ok(())
            }
            Message::Binary(bin) => {
                self.process_message(&bin, WebSocketKind::Binary, ack_tx, out)
                    .await;
                Ok(())
            }
            Message::Close(frame) => Self::handle_close_frame(frame),
            _ => Ok(()),
        }
    }

    fn handle_close_frame(frame: Option<CloseFrame<'_>>) -> Result<(), WebSocketSourceError> {
        let (error_message, specific_error) = match frame {
            Some(frame) => {
                let msg = format!(
                    "Connection closed by server with code '{}' and reason: '{}'",
                    frame.code, frame.reason
                );
                let err = WebSocketSourceError::RemoteClosed {
                    frame: frame.into_owned(),
                };
                (msg, err)
            }
            None => (
                "Connection closed by server without a close frame".to_string(),
                WebSocketSourceError::RemoteClosedEmpty,
            ),
        };

        let error = TungsteniteError::Io(std::io::Error::new(
            std::io::ErrorKind::ConnectionAborted,
            error_message,
        ));
        emit!(WebSocketReceiveError { error: &error });

        Err(specific_error)
    }

    async fn send_ack(&mut self, ws_sink: &mut WebSocketSink, message_id: impl Into<Value>) {
        if let Some(replay_config) = &self.config.replay
            && let Some(replay_ack_config) = &replay_config.ack
            && let Some(ack_serializer) = &mut self.ack_serializer
        {
            let mut value = Value::Null;
            value.insert(&replay_ack_config.message_id_path, message_id);

            let mut message_bytes = BytesMut::new();
            if let Err(err) = ack_serializer.encode(
                LogEvent::from_parts(value, EventMetadata::default()).into(),
                &mut message_bytes,
            ) {
                debug!(message = "Encoding ACK message failed.", %err);
                return;
            }
            let message = Message::binary(message_bytes);

            _ = ws_sink
                .send(message)
                .await
                .map_err(|error| {
                    emit!(WebSocketSendError { error: &error });
                    WebSocketSourceError::Tungstenite { source: error }
                })
                .inspect_err(
                    |err| warn!(message = "WebSocket error occurred while sending ACK.", %err),
                );
        }
    }

    fn is_custom_pong(&self, msg_txt: &str) -> bool {
        match self.config.pong_message.as_ref() {
            Some(config) => config.matches(msg_txt),
            None => false,
        }
    }
}

struct PingManager {
    interval: PingInterval,
    waiting_for_pong: bool,
    message: Message,
}

impl PingManager {
    fn new(config: &WebSocketConfig) -> Self {
        let ping_message = if let Some(ping_msg) = &config.ping_message {
            Message::Text(ping_msg.clone())
        } else {
            Message::Ping(vec![])
        };

        Self {
            interval: PingInterval::new(config.common.ping_interval.map(u64::from)),
            waiting_for_pong: false,
            message: ping_message,
        }
    }

    const fn record_pong(&mut self) {
        self.waiting_for_pong = false;
    }

    async fn tick(&mut self, ws_sink: &mut WebSocketSink) -> Result<(), WebSocketSourceError> {
        self.interval.tick().await;

        if self.waiting_for_pong {
            return Err(WebSocketSourceError::PongTimeout);
        }

        ws_sink.send(self.message.clone()).await.map_err(|error| {
            emit!(WebSocketSendError { error: &error });
            WebSocketSourceError::Tungstenite { source: error }
        })?;

        self.waiting_for_pong = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{borrow::Cow, num::NonZeroU64};
    use vector_lib::codecs::decoding::JsonDeserializerConfig;

    use futures::{StreamExt, sink::SinkExt};
    use tempfile::TempDir;
    use tokio::{net::TcpListener, time::Duration};
    use tokio_tungstenite::{
        accept_async, accept_hdr_async,
        tungstenite::{
            Message,
            handshake::server::{ErrorResponse, Request, Response},
            protocol::frame::{CloseFrame, coding::CloseCode},
        },
    };
    use url::Url;
    use vector_lib::codecs::{
        TextSerializerConfig, decoding::DeserializerConfig, encoding::SerializerConfig,
    };

    use crate::{
        common::websocket::WebSocketCommonConfig,
        sources::websocket::{
            config::{MessageReplayConfig, PongMessage, ReplayAckConfig, WebSocketConfig},
            source::LAST_RECEIVED_MESSAGE_ID_FILE_NAME,
        },
        test_util::{
            addr::next_addr,
            components::{
                SOURCE_TAGS, run_and_assert_source_compliance, run_and_assert_source_error,
            },
        },
    };

    fn make_config(uri: &str) -> WebSocketConfig {
        WebSocketConfig {
            common: WebSocketCommonConfig {
                uri: Url::parse(uri).unwrap().to_string(),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    /// Starts a WebSocket server that pushes a binary message to the first client.
    async fn start_binary_push_server() -> String {
        let (_guard, addr) = next_addr();
        let listener = TcpListener::bind(&addr).await.expect("Failed to bind");
        let server_addr = format!("ws://{}", listener.local_addr().unwrap());

        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut websocket = accept_async(stream).await.expect("Failed to accept");

            let binary_payload = br#"{"message": "binary data"}"#.to_vec();
            websocket
                .send(Message::Binary(binary_payload))
                .await
                .unwrap();
        });

        server_addr
    }

    /// Starts a WebSocket server that pushes a message to the first client that connects.
    async fn start_push_server() -> String {
        let (_guard, addr) = next_addr();
        let listener = TcpListener::bind(&addr).await.expect("Failed to bind");
        let server_addr = format!("ws://{}", listener.local_addr().unwrap());

        tokio::spawn(async move {
            // Accept one connection
            let (stream, _) = listener.accept().await.unwrap();
            let mut websocket = accept_async(stream).await.expect("Failed to accept");

            // Immediately send a message to the connected client (which will be our source)
            websocket
                .send(Message::Text("message from server".to_string()))
                .await
                .unwrap();
        });

        server_addr
    }

    /// Starts a WebSocket server that waits for an initial message from the client,
    /// and upon receiving it, sends a confirmation message back.
    async fn start_subscribe_server(initial_message: String, response_message: String) -> String {
        let (_guard, addr) = next_addr();
        let listener = TcpListener::bind(&addr).await.expect("Failed to bind");
        let server_addr = format!("ws://{}", listener.local_addr().unwrap());

        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut websocket = accept_async(stream).await.expect("Failed to accept");

            // Wait for the initial message from the client
            if let Some(Ok(Message::Text(msg))) = websocket.next().await
                && msg == initial_message
            {
                // Received correct initial message, send response
                websocket
                    .send(Message::Text(response_message))
                    .await
                    .unwrap();
            }
        });

        server_addr
    }

    async fn start_reconnect_server() -> String {
        let (_guard, addr) = next_addr();
        let listener = TcpListener::bind(&addr).await.expect("Failed to bind");
        let server_addr = format!("ws://{}", listener.local_addr().unwrap());

        tokio::spawn(async move {
            // First connection
            let (stream, _) = listener.accept().await.unwrap();
            let mut websocket = accept_async(stream).await.expect("Failed to accept");
            websocket
                .send(Message::Text("first message".to_string()))
                .await
                .unwrap();
            // Close the connection to force a reconnect from the client
            websocket.close(None).await.unwrap();

            // Second connection
            let (stream, _) = listener.accept().await.unwrap();
            let mut websocket = accept_async(stream).await.expect("Failed to accept");
            websocket
                .send(Message::Text("second message".to_string()))
                .await
                .unwrap();
        });

        server_addr
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn websocket_source_reconnects_after_disconnect() {
        let server_addr = start_reconnect_server().await;
        let config = make_config(&server_addr);

        // Run for a longer duration to allow for reconnection
        let events =
            run_and_assert_source_compliance(config, Duration::from_secs(5), &SOURCE_TAGS).await;

        assert_eq!(
            events.len(),
            2,
            "Should have received messages from both connections"
        );

        let event = events[0].as_log();
        assert_eq!(event["message"], "first message".into());

        let event = events[1].as_log();
        assert_eq!(event["message"], "second message".into());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn websocket_source_consume_binary_event() {
        let server_addr = start_binary_push_server().await;
        let mut config = make_config(&server_addr);
        let decoding = DeserializerConfig::Json(JsonDeserializerConfig::default());
        config.decoding = decoding;

        let events =
            run_and_assert_source_compliance(config, Duration::from_secs(2), &SOURCE_TAGS).await;

        assert!(!events.is_empty(), "No events received from source");
        let event = events[0].as_log();
        assert_eq!(event["message"], "binary data".into());
        assert_eq!(*event.get_source_type().unwrap(), "websocket".into());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn websocket_source_consume_event() {
        let server_addr = start_push_server().await;
        let config = make_config(&server_addr);

        // Run the source, which will connect to the server and receive the pushed message.
        let events =
            run_and_assert_source_compliance(config, Duration::from_secs(2), &SOURCE_TAGS).await;

        // Now assert that the event was received and is correct.
        assert!(!events.is_empty(), "No events received from source");
        let event = events[0].as_log();
        assert_eq!(event["message"], "message from server".into());
        assert_eq!(*event.get_source_type().unwrap(), "websocket".into());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn websocket_source_sends_initial_message() {
        let initial_msg = "{\"action\":\"subscribe\",\"topic\":\"test\"}".to_string();
        let response_msg = "{\"status\":\"subscribed\",\"topic\":\"test\"}".to_string();
        let server_addr = start_subscribe_server(initial_msg.clone(), response_msg.clone()).await;

        let mut config = make_config(&server_addr);
        config.initial_message = Some(initial_msg);

        let events =
            run_and_assert_source_compliance(config, Duration::from_secs(2), &SOURCE_TAGS).await;

        assert!(!events.is_empty(), "No events received from source");
        let event = events[0].as_log();
        assert_eq!(event["message"], response_msg.into());
        assert_eq!(*event.get_source_type().unwrap(), "websocket".into());
    }

    async fn start_reject_initial_message_server() -> String {
        let (_guard, addr) = next_addr();
        let listener = TcpListener::bind(&addr).await.expect("Failed to bind");
        let server_addr = format!("ws://{}", listener.local_addr().unwrap());

        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut websocket = accept_async(stream).await.expect("Failed to accept");

            if websocket.next().await.is_some() {
                let close_frame = CloseFrame {
                    code: CloseCode::Error,
                    reason: Cow::from("Simulated Internal Server Error"),
                };
                websocket.close(Some(close_frame)).await.ok(); // connection may already be gone
            }
        });

        server_addr
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn websocket_source_exits_on_rejected_initial_message() {
        let server_addr = start_reject_initial_message_server().await;

        let mut config = make_config(&server_addr);
        config.initial_message = Some("hello, server!".to_string());
        config.initial_message_timeout_secs = Duration::from_secs(1);

        run_and_assert_source_error(config, Duration::from_secs(5), &SOURCE_TAGS).await;
    }

    async fn start_unresponsive_server() -> String {
        let (_guard, addr) = next_addr();
        let listener = TcpListener::bind(&addr).await.expect("Failed to bind");
        let server_addr = format!("ws://{}", listener.local_addr().unwrap());

        tokio::spawn(async move {
            if let Ok((stream, _)) = listener.accept().await {
                // Accept the connection to establish the WebSocket.
                let mut websocket = accept_async(stream).await.expect("Failed to accept");
                // Simply wait forever without responding to pings.
                while websocket.next().await.is_some() {
                    // Do nothing
                }
            }
        });

        server_addr
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn websocket_source_exits_on_pong_timeout() {
        let server_addr = start_unresponsive_server().await;

        let mut config = make_config(&server_addr);
        config.common.ping_interval = NonZeroU64::new(3);
        config.common.ping_timeout = NonZeroU64::new(1);
        config.ping_message = Some("ping".to_string());
        config.pong_message = Some(PongMessage::Simple("pong".to_string()));

        // The source should fail because the server never sends a pong.
        run_and_assert_source_error(config, Duration::from_secs(5), &SOURCE_TAGS).await;
    }

    async fn start_blackhole_server() -> String {
        let (_guard, addr) = next_addr();
        let listener = TcpListener::bind(&addr).await.expect("Failed to bind");
        let server_addr = format!("ws://{}", listener.local_addr().unwrap());

        tokio::spawn(async move {
            let (mut _socket, _) = listener.accept().await.unwrap();
            tokio::time::sleep(Duration::from_secs(10)).await;
        });

        server_addr
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn websocket_source_exits_on_connection_timeout() {
        let server_addr = start_blackhole_server().await;
        let mut config = make_config(&server_addr);
        config.connect_timeout_secs = Duration::from_secs(1);

        run_and_assert_source_error(config, Duration::from_secs(5), &SOURCE_TAGS).await;
    }

    async fn start_ack_supported_server() -> String {
        let (_guard, addr) = next_addr();
        let listener = TcpListener::bind(&addr).await.expect("Failed to bind");
        let server_addr = format!("ws://{}", listener.local_addr().unwrap());

        tokio::spawn(async move {
            // First connection
            let (stream, _) = listener.accept().await.unwrap();
            let mut websocket = accept_async(stream).await.expect("Failed to accept");
            websocket
                .send(Message::Text("message1".to_string()))
                .await
                .unwrap();
            // Wait for ack
            let Some(Ok(Message::Text(ack))) = websocket.next().await else {
                panic!("Didn't receive ack!");
            };
            assert_eq!(ack, "message1");
            websocket
                .send(Message::Text("message2".to_string()))
                .await
                .unwrap();
            // Wait for ack
            let Some(Ok(Message::Text(ack))) = websocket.next().await else {
                panic!("Didn't receive ack!");
            };
            assert_eq!(ack, "message2");
        });

        server_addr
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn websocket_source_can_send_acks() {
        let server_addr = start_ack_supported_server().await;
        let mut config = make_config(&server_addr);
        config.replay = Some(MessageReplayConfig {
            message_id_path: ".".into(),
            ack: Some(ReplayAckConfig {
                encoding: SerializerConfig::Text(TextSerializerConfig::default()).into(),
                message_id_path: ".".into(),
            }),
            data_dir: None,
            stateful: false,
        });

        let events =
            run_and_assert_source_compliance(config, Duration::from_secs(2), &SOURCE_TAGS).await;

        assert!(!events.is_empty(), "No events received from source");
    }

    async fn start_reconnect_server_with_last_received() -> String {
        let (_guard, addr) = next_addr();
        let listener = TcpListener::bind(&addr).await.expect("Failed to bind");
        let server_addr = format!("ws://{}", listener.local_addr().unwrap());

        tokio::spawn(async move {
            // First connection
            let (stream, _) = listener.accept().await.unwrap();
            let mut websocket = accept_async(stream).await.expect("Failed to accept");
            websocket
                .send(Message::Text("message1".to_string()))
                .await
                .unwrap();
            // Close the connection to force a reconnect from the client
            websocket.close(None).await.unwrap();

            // Second connection
            let (stream, _) = listener.accept().await.unwrap();
            let mut websocket = accept_hdr_async(stream, move |req: &Request, res: Response| {
                let Some(query_params) = req.uri().query() else {
                    return Err(ErrorResponse::new(Some(
                        "missing last_received".to_string(),
                    )));
                };
                if !query_params.contains("last_received=message1") {
                    return Err(ErrorResponse::new(Some(
                        "missing last_received".to_string(),
                    )));
                }
                Ok(res)
            })
            .await
            .expect("Failed to accept");
            websocket
                .send(Message::Text("message2".to_string()))
                .await
                .unwrap();
            let _ = websocket.next().await;
        });

        server_addr
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn websocket_can_send_last_received_message_on_reconnect() {
        let server_addr = start_reconnect_server_with_last_received().await;
        let mut config = make_config(&server_addr);
        config.replay = Some(MessageReplayConfig {
            message_id_path: ".".into(),
            ack: None,
            data_dir: None,
            stateful: false,
        });

        let events =
            run_and_assert_source_compliance(config, Duration::from_secs(2), &SOURCE_TAGS).await;

        assert!(!events.is_empty(), "No events received from source");
        assert_eq!(2, events.len());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn websocket_can_persist_last_received_message_in_a_file() {
        let server_addr = start_reconnect_server_with_last_received().await;
        let mut config = make_config(&server_addr);
        let temp_dir = TempDir::new().unwrap();
        config.replay = Some(MessageReplayConfig {
            message_id_path: ".".into(),
            ack: None,
            data_dir: temp_dir.path().to_owned().into(),
            stateful: true,
        });

        let events =
            run_and_assert_source_compliance(config, Duration::from_secs(2), &SOURCE_TAGS).await;

        assert!(!events.is_empty(), "No events received from source");
        assert_eq!(events.len(), 2);

        // Check stored last_message_id
        assert_eq!(
            std::fs::read_to_string(
                temp_dir
                    .path()
                    .join("default")
                    .join(LAST_RECEIVED_MESSAGE_ID_FILE_NAME)
            )
            .unwrap(),
            "message2"
        );
    }
}
