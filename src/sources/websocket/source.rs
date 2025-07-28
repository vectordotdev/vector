use crate::vector_lib::codecs::StreamDecodingError;
use chrono::Utc;
use futures::{pin_mut, sink::SinkExt, Sink, Stream, StreamExt};
use std::num::NonZeroU64;
use std::pin::Pin;
use tokio::time;
use tokio::time::{Duration, Instant};
use tokio_tungstenite::tungstenite::{error::Error as WsError, Message};
use tokio_util::codec::FramedRead;
use vector_lib::{
    config::LogNamespace,
    event::{Event, LogEvent},
    EstimatedJsonEncodedSizeOf,
};

use crate::{
    codecs::Decoder,
    common::websocket::{is_closed, PingInterval, WebSocketConnector},
    config::SourceContext,
    internal_events::{
        ConnectionOpen, OpenGauge, WsBytesReceived, WsConnectionError, WsConnectionEstablished,
        WsConnectionShutdown, WsKind, WsMessageReceived, WsReceiveError, WsSendError, PROTOCOL,
    },
    sources::websocket::config::{PongMessage, PongValidation, WebSocketConfig},
    SourceSender,
};
use vector_lib::internal_event::{CountByteSize, EventsReceived, InternalEventHandle as _};

type WsSink = Pin<Box<dyn Sink<Message, Error = WsError> + Send>>;
type WsStream = Pin<Box<dyn Stream<Item = Result<Message, WsError>> + Send>>;

pub(crate) struct WebSocketSourceParams {
    pub connector: WebSocketConnector,
    pub decoder: Decoder,
    pub log_namespace: LogNamespace,
}

pub(crate) struct WebSocketSource {
    config: WebSocketConfig,
    params: WebSocketSourceParams,
}

impl WebSocketSource {
    pub const fn new(config: WebSocketConfig, params: WebSocketSourceParams) -> Self {
        Self { config, params }
    }

    pub async fn run(self, cx: SourceContext) -> Result<(), ()> {
        let _open_token = OpenGauge::new().open(|count| emit!(ConnectionOpen { count }));
        let mut ping_manager = PingManager::new(&self.config);

        let mut out = cx.out;

        let (ws_sink, ws_source) = self.connect(&mut out).await?;

        pin_mut!(ws_sink, ws_source);

        loop {
            let result = tokio::select! {
                _ = cx.shutdown.clone() => {
                    info!("Received shutdown signal.");
                    break;
                },

                res = ping_manager.tick(&mut ws_sink) => res,

                Some(msg_result) = ws_source.next() => {
                    match msg_result {
                        Ok(msg) => self.handle_message(msg, &mut ping_manager, &mut out).await,
                        Err(e) => Err(e.into()),
                    }
                }
            };

            if let Err(error) = result {
                if is_closed(&error) {
                    emit!(WsConnectionShutdown);
                }
                if self
                    .reconnect(&mut out, &mut ws_sink, &mut ws_source)
                    .await
                    .is_err()
                {
                    break;
                }
            }
        }
        Ok(())
    }

    async fn handle_message(
        &self,
        msg: Message,
        ping_manager: &mut PingManager,
        out: &mut SourceSender,
    ) -> Result<(), WsError> {
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
                    self.process_message(&msg_txt, WsKind::Text, out).await;
                }
                Ok(())
            }
            Message::Binary(msg_bytes) => {
                self.process_message(&msg_bytes, WsKind::Binary, out).await;
                Ok(())
            }
            Message::Ping(_) => Ok(()),
            Message::Close(_) => Err(WsError::ConnectionClosed),
            Message::Frame(_) => {
                warn!("Unsupported message type received: frame.");
                Ok(())
            }
        }
    }

    async fn process_message<T>(&self, payload: &T, kind: WsKind, out: &mut SourceSender)
    where
        T: AsRef<[u8]> + ?Sized,
    {
        let payload_bytes = payload.as_ref();

        emit!(WsBytesReceived {
            byte_size: payload_bytes.len(),
            url: &self.config.common.uri,
            protocol: PROTOCOL,
            kind,
        });
        let mut stream = FramedRead::new(payload_bytes, self.params.decoder.clone());

        while let Some(result) = stream.next().await {
            match result {
                Ok((events, _)) => {
                    if events.is_empty() {
                        continue;
                    }

                    let event_count = events.len();
                    let byte_size = events.estimated_json_encoded_size_of();

                    register!(EventsReceived).emit(CountByteSize(event_count, byte_size));
                    emit!(WsMessageReceived {
                        count: event_count,
                        byte_size,
                        url: &self.config.common.uri,
                        protocol: PROTOCOL,
                        kind,
                    });

                    let events_with_meta = events.into_iter().map(|mut event| {
                        if let Event::Log(event) = &mut event {
                            self.add_metadata(event);
                        }
                        event
                    });

                    if let Err(error) = out.send_batch(events_with_meta).await {
                        error!(message = "Error sending events.", %error);
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

    fn add_metadata(&self, event: &mut LogEvent) {
        self.params
            .log_namespace
            .insert_standard_vector_source_metadata(event, WebSocketConfig::NAME, Utc::now());
    }

    async fn send_initial_message(
        &self,
        ws_sink: &mut WsSink,
        ws_source: &mut WsStream,
        out: &mut SourceSender,
    ) -> Result<(), ()> {
        let initial_message = self.config.initial_message.as_ref().unwrap();
        ws_sink
            .send(Message::Text(initial_message.clone()))
            .await
            .map_err(|e| error!("Failed to send initial message: {e}."))?;

        debug!("Sent initial message, awaiting response from server.");

        let response_result =
            time::timeout(self.config.initial_message_timeout_secs, ws_source.next())
                .await
                .map_err(|_| error!("Server did not respond to initial message within timeout."))?
                .ok_or_else(|| error!("Connection closed after initial message without response."))?
                .map_err(|e| error!("Error waiting for server response: {e}."));

        match response_result {
            Ok(response) => self.handle_initial_response(response, out).await,
            Err(_) => Err(()),
        }
    }

    async fn handle_initial_response(
        &self,
        msg: Message,
        out: &mut SourceSender,
    ) -> Result<(), ()> {
        match msg {
            Message::Text(txt) => {
                self.process_message(&txt, WsKind::Text, out).await;
                Ok(())
            }
            Message::Binary(bin) => {
                self.process_message(&bin, WsKind::Binary, out).await;
                Ok(())
            }
            Message::Close(Some(frame)) => {
                let error = WsError::ConnectionClosed;

                emit!(WsReceiveError { error: &error });

                error!(
                    message = "Connection closed by server.",
                    code = %frame.code,
                    reason = %frame.reason,
                );

                Err(())
            }
            Message::Close(None) => {
                let error = WsError::ConnectionClosed;
                emit!(WsReceiveError { error: &error });
                error!("Connection closed without a frame.");
                Err(())
            }
            // Ignore other message types
            _ => Ok(()),
        }
    }

    async fn reconnect(
        &self,
        out: &mut SourceSender,
        ws_sink: &mut WsSink,
        ws_source: &mut WsStream,
    ) -> Result<(), ()> {
        info!("Reconnecting to WebSocket...");

        let (new_sink, new_source) = self.connect(out).await?;

        *ws_sink = new_sink;
        *ws_source = new_source;

        Ok(())
    }

    async fn connect(&self, out: &mut SourceSender) -> Result<(WsSink, WsStream), ()> {
        let (mut ws_sink, mut ws_source) = self.try_create_sink_and_stream().await?;

        if self.config.initial_message.is_some() {
            self.send_initial_message(&mut ws_sink, &mut ws_source, out)
                .await?;
        }

        Ok((ws_sink, ws_source))
    }

    async fn try_create_sink_and_stream(&self) -> Result<(WsSink, WsStream), ()> {
        let connect_future = self.params.connector.connect_backoff();
        let timeout = self.config.connect_timeout_secs;

        let ws_stream = time::timeout(timeout, connect_future).await.map_err(|_| {
            let error = WsError::Io(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "Connection attempt timed out",
            ));

            emit!(WsConnectionError { error: error });
        })?;

        emit!(WsConnectionEstablished {});
        let (sink, stream) = ws_stream.split();

        Ok((Box::pin(sink), Box::pin(stream)))
    }

    fn is_custom_pong(&self, msg_txt: &str) -> bool {
        if let Some(pong_config) = &self.config.pong_message {
            return match pong_config {
                PongMessage::Simple(expected) => msg_txt == expected,
                PongMessage::Advanced(validation) => match validation {
                    PongValidation::Exact(expected) => msg_txt == expected,
                    PongValidation::Contains(substring) => msg_txt.contains(substring),
                },
            };
        }
        false
    }
}

struct PingManager {
    interval: PingInterval,
    timeout: Option<NonZeroU64>,
    last_pong: Instant,
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
            timeout: config.common.ping_timeout,
            last_pong: Instant::now(),
            message: ping_message,
        }
    }

    fn record_pong(&mut self) {
        self.last_pong = Instant::now();
    }

    async fn tick(&mut self, ws_sink: &mut WsSink) -> Result<(), WsError> {
        self.interval.tick().await;

        if let Some(timeout) = self.timeout {
            if self.last_pong.elapsed() > Duration::from_secs(timeout.get()) {
                let error = WsError::ConnectionClosed;
                emit!(WsReceiveError { error: &error });
                return Err(error);
            }
        }

        ws_sink.send(self.message.clone()).await.map_err(|error| {
            emit!(WsSendError { error: &error });
            WsError::Io(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "Websocket connection is closed.",
            ))
        })
    }
}

#[cfg(feature = "websocket-integration-tests")]
#[cfg(test)]
mod integration_test {
    use crate::test_util::components::run_and_assert_source_error;
    use crate::{
        common::websocket::WebSocketCommonConfig,
        sources::websocket::config::WebSocketConfig,
        test_util::{
            components::{run_and_assert_source_compliance, SOURCE_TAGS},
            next_addr,
        },
    };
    use futures::{sink::SinkExt, StreamExt};
    use std::borrow::Cow;
    use std::num::NonZeroU64;
    use tokio::{net::TcpListener, time::Duration};
    use tokio_tungstenite::tungstenite::{
        protocol::frame::coding::CloseCode, protocol::frame::CloseFrame,
    };
    use tokio_tungstenite::{accept_async, tungstenite::Message};
    use url::Url;
    use vector_lib::codecs::decoding::DeserializerConfig;

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
        let addr = next_addr();
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
        let addr = next_addr();
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
        let addr = next_addr();
        let listener = TcpListener::bind(&addr).await.expect("Failed to bind");
        let server_addr = format!("ws://{}", listener.local_addr().unwrap());

        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut websocket = accept_async(stream).await.expect("Failed to accept");

            // Wait for the initial message from the client
            if let Some(Ok(Message::Text(msg))) = websocket.next().await {
                if msg == initial_message {
                    // Received correct initial message, send response
                    websocket
                        .send(Message::Text(response_message))
                        .await
                        .unwrap();
                }
            }
        });

        server_addr
    }

    async fn start_reconnect_server() -> String {
        let addr = next_addr();
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
        let decoding = DeserializerConfig::Json(Default::default());
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
        let addr = next_addr();
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
                let _ = websocket.close(Some(close_frame)).await;
            }
        });

        server_addr
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn websocket_source_exits_on_rejected_intial_messsage() {
        let server_addr = start_reject_initial_message_server().await;

        let mut config = make_config(&server_addr);
        config.initial_message = Some("hello, server!".to_string());
        config.initial_message_timeout_secs = Duration::from_secs(2);

        run_and_assert_source_error(config, Duration::from_secs(5), &SOURCE_TAGS).await;
    }

    async fn start_unresponsive_server() -> String {
        let addr = next_addr();
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
        config.common.ping_interval = NonZeroU64::new(1);
        config.common.ping_timeout = NonZeroU64::new(1);

        // The source should fail because the server never sends a pong.
        run_and_assert_source_error(config, Duration::from_secs(5), &SOURCE_TAGS).await;
    }

    async fn start_blackhole_server() -> String {
        let addr = next_addr();
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
}
