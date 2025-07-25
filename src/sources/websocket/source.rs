use crate::vector_lib::codecs::StreamDecodingError;
use chrono::Utc;
use futures::{pin_mut, sink::SinkExt, Sink, Stream, StreamExt};
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
        ConnectionOpen, OpenGauge, PongTimeoutError, WsBytesReceived, WsConnectionShutdown, WsKind,
        WsMessageReceived, WsReceiveError, WsSendError, PROTOCOL,
    },
    sources::websocket::config::{PongMessage, PongValidation, WebSocketConfig},
    SourceSender,
};
use vector_lib::internal_event::{CountByteSize, EventsReceived, InternalEventHandle as _};

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
        let (mut ws_sink, ws_source) = self.create_sink_and_stream().await;
        self.maybe_send_initial_message(&mut ws_sink).await;

        pin_mut!(ws_sink, ws_source);

        let _open_token = OpenGauge::new().open(|count| emit!(ConnectionOpen { count }));
        let ping_message = if let Some(ping_msg) = &self.config.ping_message {
            Message::Text(ping_msg.clone())
        } else {
            Message::Ping(vec![])
        };

        let mut ping = PingInterval::new(self.config.common.ping_interval.map(u64::from));
        let mut last_pong = Instant::now();

        let mut out = cx.out;

        loop {
            let result = tokio::select! {
                _ = cx.shutdown.clone() => {
                    info!("Received shutdown signal.");
                    break;
                },

                _ = ping.tick() => {
                    self.handle_ping_tick(&mut ws_sink, last_pong, &ping_message).await
                },

                Some(msg) = ws_source.next() => {
                    match msg {
                        Ok(Message::Pong(_)) => {
                            last_pong = Instant::now();
                            Ok(())
                        },
                        Ok(Message::Text(msg_txt)) => {
                            if self.is_custom_pong(&msg_txt) {
                                last_pong = Instant::now();
                                debug!("Received custom pong response.");
                            } else {
                                self.handle_message(&msg_txt, WsKind::Text, &mut out).await;
                            }
                            Ok(())
                        },
                        Ok(Message::Binary(msg_bytes)) => {
                            self.handle_message(&msg_bytes, WsKind::Binary, &mut out).await;
                            Ok(())
                        },
                        Ok(Message::Ping(_)) => Ok(()),
                        Ok(Message::Close(_)) => Err(WsError::ConnectionClosed),
                        Ok(Message::Frame(_)) => {
                            warn!("Unsupported message type received: frame.");
                             Ok(())
                        }
                        Err(e) => Err(e),
                    }
                }
            };

            if let Err(error) = result {
                if is_closed(&error) {
                    emit!(WsConnectionShutdown);
                    return Err(());
                }

                emit!(WsReceiveError { error });

                // Reconnection logic
                let (new_sink, new_source) = self.create_sink_and_stream().await;
                ws_sink.set(new_sink);
                ws_source.set(new_source);

                self.maybe_send_initial_message(&mut ws_sink).await;
            }
        }

        Ok(())
    }

    async fn handle_message<T>(&self, payload: &T, kind: WsKind, out: &mut SourceSender)
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
        self.process_payload(payload_bytes, kind, out).await;
    }

    async fn process_payload(&self, payload: &[u8], kind: WsKind, out: &mut SourceSender) {
        let mut stream = FramedRead::new(payload, self.params.decoder.clone());

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

    async fn maybe_send_initial_message(
        &self,
        ws_sink: &mut (impl Sink<Message, Error = WsError> + Unpin),
    ) {
        if let Some(initial_message) = &self.config.initial_message {
            match ws_sink.send(Message::Text(initial_message.clone())).await {
                Ok(_) => debug!(message = %initial_message, "Sent initial message."),
                Err(error) => {
                    emit!(WsSendError { error });
                    // Avoid a tight loop if sending the initial message fails repeatedly.
                    time::sleep(Duration::from_secs(1)).await;
                }
            }
        }
    }

    async fn create_sink_and_stream(
        &self,
    ) -> (
        impl Sink<Message, Error = WsError>,
        impl Stream<Item = Result<Message, WsError>>,
    ) {
        let ws_stream = self.params.connector.connect_backoff().await;
        ws_stream.split()
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

    async fn handle_ping_tick(
        &self,
        ws_sink: &mut (impl Sink<Message, Error = WsError> + Unpin),
        last_pong: Instant,
        ping_message: &Message,
    ) -> Result<(), WsError> {
        let ping_timeout = self.config.common.ping_timeout;

        if let Some(ping_timeout) = ping_timeout {
            if last_pong.elapsed() > Duration::from_secs(ping_timeout.into()) {
                let error =
                    std::io::Error::new(std::io::ErrorKind::TimedOut, "Pong not received in time.");
                emit!(PongTimeoutError {
                    timeout_secs: ping_timeout,
                });
                return Err(WsError::Io(error));
            }
        }

        ws_sink.send(ping_message.clone()).await.map_err(|error| {
            emit!(WsSendError { error });
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
    use crate::{
        common::websocket::WebSocketCommonConfig,
        sources::websocket::config::WebSocketConfig,
        test_util::{
            components::{run_and_assert_source_compliance, SOURCE_TAGS},
            next_addr,
        },
    };
    use futures::{sink::SinkExt, StreamExt};
    use tokio::{net::TcpListener, time::Duration};
    use tokio_tungstenite::{accept_async, tungstenite::Message};
    use url::Url;

    fn make_config(uri: &str) -> WebSocketConfig {
        WebSocketConfig {
            common: WebSocketCommonConfig {
                uri: Url::parse(uri).unwrap().to_string(),
                ..Default::default()
            },
            ..Default::default()
        }
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
}
