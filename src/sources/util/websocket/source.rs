use std::{io, num::NonZeroU64};

use bytes::BytesMut;
use chrono::Utc;
use futures::{pin_mut, sink::SinkExt, Sink, Stream, StreamExt};
use lookup::{metadata_path, path};
use tokio::time::{Duration, Instant};
use tokio_tungstenite::tungstenite::{error::Error as WsError, Message};
use tokio_util::codec::Decoder as DecoderTrait;

use crate::{
    codecs::Decoder,
    common::{
        ping::PingInterval,
        websocket::{is_closed, WebSocketConnector},
    },
    config::SourceContext,
    internal_events::{
        ConnectionOpen, OpenGauge, WsConnectionError, WsConnectionShutdown, WsMessageReceived,
    },
    SourceSender,
};

use vector_core::config::{LegacyKey, LogNamespace}; //, SourceOutput};
use vector_core::event::{Event, EventArray, LogEvent};

const DEFAULT_SOURCE_NAME: &str = "websocket";

#[derive(Debug, PartialEq)]
struct WebSocketEvent<'a> {
    name: String,
    payload: String,
    log_namespace: &'a LogNamespace,
}

impl From<WebSocketEvent<'_>> for LogEvent {
    fn from(frame: WebSocketEvent) -> LogEvent {
        let WebSocketEvent {
            name,
            payload,
            log_namespace,
        } = frame;

        let mut log = LogEvent::default();

        if let LogNamespace::Vector = log_namespace {
            log.insert(metadata_path!("vector", "ingest_timestamp"), Utc::now());
        }

        log_namespace.insert_source_metadata(
            &name,
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
    const PONG: &[u8] = b"PONG";
    let ping_interval = config.ping_interval;
    let ping_timeout = config.ping_timeout;

    let (ws_sink, ws_source) = create_sink_and_stream(params.connector.clone()).await;
    pin_mut!(ws_sink);
    pin_mut!(ws_source);

    let _open_token = OpenGauge::new().open(|count| emit!(ConnectionOpen { count }));

    // tokio::time::Interval panics if the period arg is zero. Since the struct members are
    // using NonZeroU64 that is not something we need to account for.
    let mut ping = PingInterval::new(ping_interval.map(u64::from));

    if let Some(_) = ping_interval {
        if let Err(error) = ws_sink.send(Message::Ping(PING.to_vec())).await {
            emit!(WsConnectionError { error });
            return Err(());
        }
    }
    let mut last_pong = Instant::now();

    let out = cx.out.clone();

    loop {
        let result = tokio::select! {
            _ = cx.shutdown.clone() => {
                info!("Received shutdown signal");
                break;
            },

            _ = ping.tick() => {
                match check_received_pong_time(ping_timeout, last_pong) {
                    Ok(()) => ws_sink.send(Message::Ping(PING.to_vec())).await.map(|_| ()),
                    Err(e) => Err(e)
                }
            },

            Some(msg) = ws_source.next() => {
                // Pongs are sent automatically by tungstenite during reading from the stream.
                match msg {
                    Ok(Message::Ping(_)) => {
                        if let Err(error) = ws_sink.send(Message::Pong(PONG.to_vec())).await {
                            Err(error)
                        } else {
                            Ok(())
                        }
                    },

                    Ok(Message::Pong(_)) => {
                        last_pong = Instant::now();
                        Ok(())
                    },

                    Ok(Message::Text(msg_txt)) => {
                        Ok(handle_text_message(&mut out.clone(), WebSocketEvent{
                            name: DEFAULT_SOURCE_NAME.to_owned(),
                            payload: msg_txt,
                            log_namespace: &params.log_namespace,
                        }, config.uri.clone(),
                        ).await)
                    },

                    Ok(Message::Binary(msg_bytes)) => {
                        let mut buf: BytesMut = msg_bytes.iter().collect();
                        match params.decoder.clone().decode(&mut buf)
                            .map(|maybe_msg| async {
                                maybe_msg.and_then(|(msg, _)| {
                                    msg.into_iter().nth(0)
                                }).and_then(|e| {
                                    if let Event::Log(log_evt) = e {
                                        Some(WebSocketEvent{
                                            name: DEFAULT_SOURCE_NAME.to_owned(),
                                            payload: log_evt.value().to_string(),
                                            log_namespace: &params.log_namespace,
                                        })
                                    } else {
                                        warn!("Decoded unsupported event: {:?}", e);
                                        None
                                    }
                                }).map(|evt| async {
                                    handle_text_message(&mut out.clone(), evt, config.uri.clone()).await
                                }).ok_or(())?.await;
                                Ok::<(), ()>(())
                            }).map_err(|err| {error!("Failed to process binary message: {}", err);}) {
                                Ok(_) => Ok(()),
                                Err(e) => {
                                    error!("Failed to send binary message: {:?}", e);
                                    Ok(())
                                }
                            }
                    },

                    Ok(Message::Close(_)) => {
                        info!("Received message: connection closed from server");
                        Err(WsError::ConnectionClosed)
                    },

                    Ok(Message::Frame(_)) => {
                        warn!("Unsupported message type received: frame");
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
                emit!(WsConnectionError { error });
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
            return Err(WsError::Io(io::Error::new(
                io::ErrorKind::TimedOut,
                "Pong not received in time",
            )));
        }
    }

    Ok(())
}

async fn handle_text_message<'a>(
    out: &mut SourceSender,
    msg: WebSocketEvent<'a>,
    endpoint: String,
) -> () {
    emit!(WsMessageReceived { url: endpoint });

    if let Err(error) = out.send_event(EventArray::Logs(vec![msg.into()])).await {
        error!("Could not send events: {}", error);
    }
}
