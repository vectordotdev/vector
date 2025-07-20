use bytes::Bytes;
use std::{io, num::NonZeroU64};
use vector_common::json_size::JsonSize;
use vector_lib::{
    config::{log_schema, LegacyKey, LogNamespace},
    event::{Event, EventArray, EventContainer, LogEvent},
    lookup::{metadata_path, path},
    EstimatedJsonEncodedSizeOf,
};

use bytes::BytesMut;
use chrono::Utc;
use futures::{pin_mut, sink::SinkExt, Sink, Stream, StreamExt};
use tokio::time::{Duration, Instant};
use tokio_tungstenite::tungstenite::{error::Error as WsError, Message};
use tokio_util::codec::Decoder as DecoderTrait;

use crate::{
    codecs::Decoder,
    common::websocket::{is_closed, PingInterval, WebSocketConnector},
    config::SourceContext,
    internal_events::{
        ConnectionOpen, OpenGauge, WsBytesReceived, WsConnectionShutdown,
        WsKind, WsMessageReceived, PROTOCOL, WsReceiveError, WsSendError, WsBinaryDecodeError,
        PongTimeoutError
    },
    SourceSender,
};
use vector_lib::{
    internal_event::{CountByteSize, EventsReceived, InternalEventHandle as _},
};


#[derive(Debug, PartialEq)]
struct WebSocketEvent<'a> {
    payload: String,
    log_namespace: &'a LogNamespace,
}

impl WebSocketEvent<'_> {
    const NAME: &'static str = "websocket";
}

impl From<WebSocketEvent<'_>> for LogEvent {
    fn from(frame: WebSocketEvent) -> LogEvent {
        let WebSocketEvent {
            payload,
            log_namespace,
        } = frame;

        let mut log = LogEvent::default();

        log_namespace.insert_vector_metadata(
            &mut log,
            log_schema().source_type_key(),
            path!("source_type"),
            Bytes::from_static(WebSocketEvent::NAME.as_bytes()),
        );

        if let LogNamespace::Vector = log_namespace {
            log.insert(metadata_path!("vector", "ingest_timestamp"), Utc::now());
        }

        log_namespace.insert_source_metadata(
            WebSocketEvent::NAME,
            &mut log,
            Some(LegacyKey::Overwrite(path!("payload"))),
            path!("payload"),
            payload,
        );

        log
    }
}

impl From<WebSocketEvent<'_>> for Event {
    fn from(frame: WebSocketEvent) -> Event {
        LogEvent::from(frame).into()
    }
}

impl EstimatedJsonEncodedSizeOf for WebSocketEvent<'_> {
    fn estimated_json_encoded_size_of(&self) -> JsonSize {
        self.payload.estimated_json_encoded_size_of()
    }
}

pub(crate) struct WebSocketSourceParams {
    pub connector: WebSocketConnector,
    pub decoder: Decoder,
    pub log_namespace: LogNamespace,
}

pub(crate) async fn recv_from_websocket(
    cx: SourceContext,
    config: super::config::WebSocketConfig,
    params: WebSocketSourceParams,
) -> Result<(), ()> {
    const PING: &[u8] = b"PING";
    let ping_interval = config.common.ping_interval;
    let ping_timeout = config.common.ping_timeout;

    let (ws_sink, ws_source) = create_sink_and_stream(params.connector.clone()).await;
    pin_mut!(ws_sink);
    pin_mut!(ws_source);

    let _open_token = OpenGauge::new().open(|count| emit!(ConnectionOpen { count }));

    // tokio::time::Interval panics if the period arg is zero. Since the struct members are
    // using NonZeroU64 that is not something we need to account for.
    let mut ping = PingInterval::new(ping_interval.map(u64::from));

    if ping_interval.is_some() {
        if let Err(error) = ws_sink.send(Message::Ping(PING.to_vec())).await {
            emit!(WsSendError { error });
            return Err(());
        }
    }
    let mut last_pong = Instant::now();

    let out = cx.out.clone();

    loop {
        let result = tokio::select! {
            _ = cx.shutdown.clone() => {
                info!("Received shutdown signal.");
                break;
            },

            _ = ping.tick() => {
                match check_received_pong_time(ping_timeout, last_pong) {
                    Ok(()) => ws_sink.send(Message::Ping(PING.to_vec())).await.map_err(|error| {
                        emit!(WsSendError { error });
                        WsError::Io(io::Error::new(io::ErrorKind::BrokenPipe, "Websocket connection is closed."))
                    }),
                    Err(e) => Err(e)
                }
            },

            Some(msg) = ws_source.next() => {

                // Pongs are sent automatically by tungstenite during reading from the stream.
                match msg {
                    Ok(Message::Ping(ping)) => {
                        emit!(WsBytesReceived{
                            byte_size: ping.len(),
                            url: &config.common.uri,
                            protocol: PROTOCOL,
                            kind: WsKind::Ping,
                        });

                        Ok(())
                    },

                    Ok(Message::Pong(_)) => {
                        last_pong = Instant::now();
                        Ok(())
                    },

                    Ok(Message::Text(msg_txt)) => {
                        emit!(WsBytesReceived{
                            byte_size: msg_txt.len(),
                            url: &config.common.uri,
                            protocol: PROTOCOL,
                            kind: WsKind::Text,
                        });

                    handle_message(
                        &mut out.clone(),
                        WebSocketEvent{
                            payload: msg_txt,
                            log_namespace: &params.log_namespace,
                        },
                        &config.common.uri,
                        ).await;

                        Ok(())
                    },

                    Ok(Message::Binary(msg_bytes)) => {
                        emit!(WsBytesReceived{
                            byte_size: msg_bytes.len(),
                            url: &config.common.uri,
                            protocol: PROTOCOL,
                            kind: WsKind::Binary,
                        });

                        handle_binary_payload(msg_bytes, &params, &config.common.uri, out.clone()).await
                    },

                    Ok(Message::Close(_)) => {
                        info!("Received message: connection closed from server.");
                        Err(WsError::ConnectionClosed)
                    },

                    Ok(Message::Frame(_)) => {
                        warn!("Unsupported message type received: frame.");
                        Ok(())
                    },

                    Err(e) => Err(e),
                }
            }
        };

        if let Err(error) = result {
            if is_closed(&error) {
                emit!(WsConnectionShutdown);
                return Err(());
            } else {
                emit!(WsReceiveError { error });
                (*ws_sink, *ws_source) = create_sink_and_stream(params.connector.clone()).await;
            }
        }
    }

    Ok(())
}

async fn create_sink_and_stream(
    connector: WebSocketConnector,
) -> (
    impl Sink<Message, Error = WsError>,
    impl Stream<Item = Result<Message, WsError>>,
) {
    let ws_stream = connector.connect_backoff().await;
    ws_stream.split()
}

fn check_received_pong_time(
    ping_timeout: Option<NonZeroU64>,
    last_pong: Instant,
) -> Result<(), WsError> {
    if let Some(ping_timeout) = ping_timeout {
        if last_pong.elapsed() > Duration::from_secs(ping_timeout.into()) {
            let error = io::Error::new(
                io::ErrorKind::TimedOut,
                "Pong not received in time.",
            );
            emit!(PongTimeoutError{
                timeout_secs: ping_timeout,
            });
            return Err(WsError::Io(error));
        }
    }

    Ok(())
}

async fn handle_message<'a>(out: &mut SourceSender, msg: WebSocketEvent<'a>, endpoint: &str) {
    let events_received = register!(EventsReceived);

    let json_size = msg.estimated_json_encoded_size_of();
    let events = EventArray::Logs(vec![msg.into()]);

    events_received.emit(CountByteSize(
        events.len(),
        json_size,
    ));

    emit!(WsMessageReceived {
        count: events.len(),
        byte_size: json_size,
        url: endpoint,
        protocol: WebSocketEvent::NAME,
        kind: WsKind::Text,
    });

    if let Err(error) = out.send_event(events).await {
        error!("Could not send events: {}.", error);
    }
}

async fn handle_binary_payload(
    msg_bytes: Vec<u8>,
    params: &WebSocketSourceParams,
    uri: &str,
    mut out: SourceSender,
) -> Result<(), WsError> {
    let mut buf: BytesMut = msg_bytes.iter().collect();
    match params
        .decoder
        .clone()
        .decode(&mut buf)
        .map(|maybe_msg| async {
            maybe_msg
                .and_then(|(msg, _)| msg.into_iter().next())
                .and_then(|e| {
                    if let Event::Log(log_evt) = e {
                        Some(WebSocketEvent {
                            payload: log_evt.value().to_string(),
                            log_namespace: &params.log_namespace,
                        })
                    } else {
                        warn!("Decoded unsupported event: {:?}.", e);
                        None
                    }
                })
                .map(|evt| async { handle_message(&mut out, evt, uri).await })
                .ok_or(())?
                .await;
            Ok::<(), ()>(())
        })
        .map_err(|error| {
            emit!(WsBinaryDecodeError { error });
        }) {
        Ok(_) => Ok(()),
        Err(e) => {
            // This case should ideally not be reached since map_err should handle it.
            // However, to be safe, we emit a generic error here.
            error!("Failed to send binary message: {:?}.", e);
            Ok(())
        }
    }
}
#[cfg(feature = "websocket-integration-tests")]
#[cfg(test)]
mod integration_test {
    use crate::{
        test_util::{
            components::{run_and_assert_source_compliance, SOURCE_TAGS},
            next_addr,
        },
    };
    use crate::sources::websocket::config::WebSocketConfig;
    use crate::common::websocket::WebSocketCommonConfig;
    use futures::sink::SinkExt;
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

    #[tokio::test(flavor = "multi_thread")]
    async fn websocket_source_consume_event() {
        let server_addr = start_push_server().await;
        let config = make_config(&server_addr);

        // Run the source, which will connect to the server and receive the pushed message.
        let events =
            run_and_assert_source_compliance(config, Duration::from_secs(2), &SOURCE_TAGS).await;
        
        // Now assert that the event was received and is correct.
        assert!(!events.is_empty(), "No events received from source");
        let log = events[0].as_log();
        assert_eq!(log["payload"], "message from server".into());
        assert_eq!(*log.get_source_type().unwrap(), "websocket".into());
    }
}
