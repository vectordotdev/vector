use std::pin::Pin;

use chrono::{DateTime, Utc};
use futures::{Sink, Stream, StreamExt, pin_mut, sink::SinkExt};
use snafu::Snafu;
use tokio::time;
use tokio_util::codec::FramedRead;
use vector_lib::{
    EstimatedJsonEncodedSizeOf,
    config::LogNamespace,
    event::{Event, LogEvent},
    internal_event::{CountByteSize, EventsReceived, InternalEventHandle as _},
};
use yawc::{Frame, OpCode, WebSocketError, close::CloseCode};

use crate::{
    SourceSender,
    codecs::Decoder,
    common::websocket::{PingInterval, WebSocketConnector},
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

type WsSink = Pin<Box<dyn Sink<Frame, Error = WebSocketError> + Send>>;
type WsStream = Pin<Box<dyn Stream<Item = Frame> + Send>>;

pub(crate) struct WebSocketSourceParams {
    pub connector: WebSocketConnector,
    pub decoder: Decoder,
    pub log_namespace: LogNamespace,
}

pub(crate) struct WebSocketSource {
    config: WebSocketConfig,
    params: WebSocketSourceParams,
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

    #[snafu(display("Connection closed by server with code '{:?}' and reason: '{}'", code, reason))]
    RemoteClosed {
        code: CloseCode,
        reason: String,
    },

    #[snafu(display("Connection closed by server without a close frame"))]
    RemoteClosedEmpty,

    #[snafu(display("Connection timed out while waiting for a pong response"))]
    PongTimeout,

    #[snafu(display("A WebSocket error occurred: {}", source))]
    WebSocket { source: WebSocketError },
}

impl WebSocketSource {
    pub const fn new(config: WebSocketConfig, params: WebSocketSourceParams) -> Self {
        Self { config, params }
    }

    pub async fn run(self, cx: SourceContext) -> Result<(), WebSocketSourceError> {
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

                maybe_frame = ws_source.next() => {
                    match maybe_frame {
                        Some(frame) => self.handle_message(frame, &mut ping_manager, &mut out).await,
                        None => {
                            // Stream ended — connection lost or closed
                            Err(WebSocketSourceError::RemoteClosedEmpty)
                        }
                    }
                }
            };

            if let Err(error) = result {
                match error {
                    WebSocketSourceError::RemoteClosed { ref code, ref reason } => {
                        warn!(
                            message = "Connection closed by server.",
                            code = ?code,
                            reason = %reason
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
                            error: &WebSocketError::ConnectionClosed
                        });
                        emit!(WebSocketConnectionShutdown);
                        return Err(error);
                    }
                    WebSocketSourceError::WebSocket { source: ref ws_err } => {
                        if ws_err.is_closed() {
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
                            "Encountered a connection-time error during runtime: {:?}",
                            error
                        );
                    }
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
        frame: Frame,
        ping_manager: &mut PingManager,
        out: &mut SourceSender,
    ) -> Result<(), WebSocketSourceError> {
        match frame.opcode() {
            OpCode::Pong => {
                ping_manager.record_pong();
                Ok(())
            }
            OpCode::Text => {
                let text = std::str::from_utf8(frame.payload()).unwrap_or("");
                if self.is_custom_pong(text) {
                    ping_manager.record_pong();
                    debug!("Received custom pong response.");
                } else {
                    self.process_message(frame.payload(), WebSocketKind::Text, out)
                        .await;
                }
                Ok(())
            }
            OpCode::Binary => {
                self.process_message(frame.payload(), WebSocketKind::Binary, out)
                    .await;
                Ok(())
            }
            OpCode::Ping => Ok(()),
            OpCode::Close => self.handle_close_frame(&frame),
            _ => {
                warn!("Unsupported message type received.");
                Ok(())
            }
        }
    }

    async fn process_message<T>(&self, payload: &T, kind: WebSocketKind, out: &mut SourceSender)
    where
        T: AsRef<[u8]> + ?Sized,
    {
        let payload_bytes = payload.as_ref();

        emit!(WebSocketBytesReceived {
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
                    emit!(WebSocketMessageReceived {
                        count: event_count,
                        byte_size,
                        url: &self.config.common.uri,
                        protocol: PROTOCOL,
                        kind,
                    });

                    let now = Utc::now();
                    let events_with_meta = events.into_iter().map(|mut event| {
                        if let Event::Log(event) = &mut event {
                            self.add_metadata(event, now);
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

    fn add_metadata(&self, event: &mut LogEvent, now: DateTime<Utc>) {
        self.params
            .log_namespace
            .insert_standard_vector_source_metadata(event, WebSocketConfig::NAME, now);
    }

    async fn reconnect(
        &self,
        out: &mut SourceSender,
        ws_sink: &mut WsSink,
        ws_source: &mut WsStream,
    ) -> Result<(), WebSocketSourceError> {
        info!("Reconnecting to WebSocket...");

        let (new_sink, new_source) = self.connect(out).await?;

        *ws_sink = new_sink;
        *ws_source = new_source;

        info!("Reconnected to Websocket.");

        Ok(())
    }

    async fn connect(
        &self,
        out: &mut SourceSender,
    ) -> Result<(WsSink, WsStream), WebSocketSourceError> {
        let (mut ws_sink, mut ws_source) = self.try_create_sink_and_stream().await?;

        if self.config.initial_message.is_some() {
            self.send_initial_message(&mut ws_sink, &mut ws_source, out)
                .await?;
        }

        Ok((ws_sink, ws_source))
    }

    async fn try_create_sink_and_stream(
        &self,
    ) -> Result<(WsSink, WsStream), WebSocketSourceError> {
        let ws_stream = self
            .params
            .connector
            .connect_backoff_with_timeout(self.config.connect_timeout_secs)
            .await;

        let (sink, stream) = ws_stream.split();

        Ok((Box::pin(sink), Box::pin(stream)))
    }

    async fn send_initial_message(
        &self,
        ws_sink: &mut WsSink,
        ws_source: &mut WsStream,
        out: &mut SourceSender,
    ) -> Result<(), WebSocketSourceError> {
        let initial_message = self.config.initial_message.as_ref().unwrap();
        ws_sink
            .send(Frame::text(initial_message.to_string()))
            .await
            .map_err(|error| {
                emit!(WebSocketSendError { error: &error });
                WebSocketSourceError::WebSocket { source: error }
            })?;

        debug!("Sent initial message, awaiting response from server.");

        let frame =
            match time::timeout(self.config.initial_message_timeout_secs, ws_source.next()).await {
                Ok(Some(frame)) => frame,
                Ok(None) => fail_with_event!(ConnectionClosedPrematurelySnafu),
                Err(_) => fail_with_event!(InitialMessageTimeoutSnafu),
            };

        match frame.opcode() {
            OpCode::Text => {
                self.process_message(frame.payload(), WebSocketKind::Text, out)
                    .await;
                Ok(())
            }
            OpCode::Binary => {
                self.process_message(frame.payload(), WebSocketKind::Binary, out)
                    .await;
                Ok(())
            }
            OpCode::Close => self.handle_close_frame(&frame),
            _ => Ok(()),
        }
    }

    fn handle_close_frame(
        &self,
        frame: &Frame,
    ) -> Result<(), WebSocketSourceError> {
        let close_code = frame.close_code();
        let close_reason = frame.close_reason().ok().flatten().unwrap_or("");

        let specific_error = match close_code {
            Some(code) => WebSocketSourceError::RemoteClosed {
                code,
                reason: close_reason.to_string(),
            },
            None => WebSocketSourceError::RemoteClosedEmpty,
        };

        emit!(WebSocketReceiveError {
            error: &WebSocketError::ConnectionClosed
        });

        Err(specific_error)
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
    message: Frame,
}

impl PingManager {
    fn new(config: &WebSocketConfig) -> Self {
        let ping_message = if let Some(ping_msg) = &config.ping_message {
            Frame::text(ping_msg.clone())
        } else {
            Frame::ping(b"" as &[u8])
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

    async fn tick(&mut self, ws_sink: &mut WsSink) -> Result<(), WebSocketSourceError> {
        self.interval.tick().await;

        if self.waiting_for_pong {
            return Err(WebSocketSourceError::PongTimeout);
        }

        ws_sink.send(self.message.clone()).await.map_err(|error| {
            emit!(WebSocketSendError { error: &error });
            WebSocketSourceError::WebSocket { source: error }
        })?;

        self.waiting_for_pong = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU64;

    use bytes::Bytes;
    use futures::{StreamExt, sink::SinkExt};
    use hyper1::{body::Incoming, service::service_fn};
    use hyper_util::rt::TokioIo;
    use tokio::{net::TcpListener, time::Duration};
    use url::Url;
    use vector_lib::codecs::decoding::DeserializerConfig;
    use yawc::{Frame, OpCode, WebSocket as YawcWebSocket, close::CloseCode};

    use crate::{
        common::websocket::WebSocketCommonConfig,
        sources::websocket::config::{PongMessage, WebSocketConfig},
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

    /// Helper: accept a TCP connection via hyper1 HTTP upgrade and return a yawc WebSocket.
    async fn accept_ws(listener: &TcpListener) -> yawc::WebSocket<yawc::HttpStream> {
        let (stream, _) = listener.accept().await.unwrap();
        let io = TokioIo::new(stream);

        let (tx, rx) = tokio::sync::oneshot::channel();
        let tx = std::sync::Mutex::new(Some(tx));

        let service = service_fn(move |mut req: hyper1::Request<Incoming>| {
            let tx = tx.lock().unwrap().take();
            async move {
                let (response, upgrade_fut) =
                    YawcWebSocket::upgrade(&mut req).expect("upgrade failed");

                if let Some(tx) = tx {
                    let _ = tx.send(upgrade_fut);
                }

                Ok::<_, hyper1::Error>(response)
            }
        });

        tokio::spawn(async move {
            let _ = hyper1::server::conn::http1::Builder::new()
                .serve_connection(io, service)
                .with_upgrades()
                .await;
        });

        rx.await.unwrap().await.unwrap()
    }

    /// Starts a WebSocket server that pushes a binary message to the first client.
    async fn start_binary_push_server() -> String {
        let (_guard, addr) = next_addr();
        let listener = TcpListener::bind(&addr).await.expect("Failed to bind");
        let server_addr = format!("ws://{}", listener.local_addr().unwrap());

        tokio::spawn(async move {
            let mut websocket = accept_ws(&listener).await;

            let binary_payload = br#"{"message": "binary data"}"#;
            websocket
                .send(Frame::binary(Bytes::from_static(binary_payload)))
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
            let mut websocket = accept_ws(&listener).await;

            websocket
                .send(Frame::text("message from server"))
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
            let mut websocket = accept_ws(&listener).await;

            // Wait for the initial message from the client
            if let Some(frame) = websocket.next().await
                && frame.opcode() == OpCode::Text
                && std::str::from_utf8(frame.payload()).unwrap_or("") == initial_message
            {
                websocket
                    .send(Frame::text(response_message.clone()))
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
            let mut websocket = accept_ws(&listener).await;
            websocket
                .send(Frame::text("first message"))
                .await
                .unwrap();
            // Close the connection to force a reconnect from the client
            websocket
                .send(Frame::close(CloseCode::Normal, b""))
                .await
                .unwrap();

            // Second connection
            let mut websocket = accept_ws(&listener).await;
            websocket
                .send(Frame::text("second message"))
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
        let (_guard, addr) = next_addr();
        let listener = TcpListener::bind(&addr).await.expect("Failed to bind");
        let server_addr = format!("ws://{}", listener.local_addr().unwrap());

        tokio::spawn(async move {
            let mut websocket = accept_ws(&listener).await;

            if websocket.next().await.is_some() {
                let _ = websocket
                    .send(Frame::close(CloseCode::Error, b"Simulated Internal Server Error"))
                    .await;
            }
        });

        server_addr
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn websocket_source_exits_on_rejected_intial_messsage() {
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
            let mut websocket = accept_ws(&listener).await;
            // Simply wait forever without responding to pings.
            while websocket.next().await.is_some() {
                // Do nothing
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
}
