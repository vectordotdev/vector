use std::{
    io,
    num::NonZeroU64,
    time::{Duration, Instant},
};

use async_trait::async_trait;
use bytes::BytesMut;
use futures::{pin_mut, sink::SinkExt, stream::BoxStream, Sink, Stream, StreamExt};
use tokio_tungstenite::tungstenite::{error::Error as TungsteniteError, protocol::Message};
use tokio_util::codec::Encoder as _;
use vector_lib::{
    emit,
    internal_event::{
        ByteSize, BytesSent, CountByteSize, EventsSent, InternalEventHandle as _, Output, Protocol,
    },
    EstimatedJsonEncodedSizeOf,
};

use crate::{
    codecs::{Encoder, Transformer},
    common::websocket::{is_closed, PingInterval, WebSocketConnector},
    event::{Event, EventStatus, Finalizable},
    internal_events::{
        ConnectionOpen, OpenGauge, WebSocketConnectionError, WebSocketConnectionShutdown,
    },
    sinks::util::StreamSink,
    sinks::websocket::config::WebSocketSinkConfig,
};

pub struct WebSocketSink {
    transformer: Transformer,
    encoder: Encoder<()>,
    connector: WebSocketConnector,
    ping_interval: Option<NonZeroU64>,
    ping_timeout: Option<NonZeroU64>,
}

impl WebSocketSink {
    pub(crate) fn new(
        config: &WebSocketSinkConfig,
        connector: WebSocketConnector,
    ) -> crate::Result<Self> {
        let transformer = config.encoding.transformer();
        let serializer = config.encoding.build()?;
        let encoder = Encoder::<()>::new(serializer);

        Ok(Self {
            transformer,
            encoder,
            connector,
            ping_interval: config.common.ping_interval,
            ping_timeout: config.common.ping_timeout,
        })
    }

    async fn create_sink_and_stream(
        &self,
    ) -> (
        impl Sink<Message, Error = TungsteniteError>,
        impl Stream<Item = Result<Message, TungsteniteError>>,
    ) {
        let ws_stream = self.connector.connect_backoff().await;
        ws_stream.split()
    }

    fn check_received_pong_time(&self, last_pong: Instant) -> Result<(), TungsteniteError> {
        if let Some(ping_timeout) = self.ping_timeout {
            if last_pong.elapsed() > Duration::from_secs(ping_timeout.into()) {
                return Err(TungsteniteError::Io(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Pong not received in time",
                )));
            }
        }

        Ok(())
    }

    const fn should_encode_as_binary(&self) -> bool {
        use vector_lib::codecs::encoding::Serializer::{
            Avro, Cef, Csv, Gelf, Json, Logfmt, Native, NativeJson, Protobuf, RawMessage, Text,
        };

        match self.encoder.serializer() {
            RawMessage(_) | Avro(_) | Native(_) | Protobuf(_) => true,
            Cef(_) | Csv(_) | Logfmt(_) | Gelf(_) | Json(_) | Text(_) | NativeJson(_) => false,
        }
    }

    async fn handle_events<I, WS, O>(
        &mut self,
        input: &mut I,
        ws_stream: &mut WS,
        ws_sink: &mut O,
    ) -> Result<(), ()>
    where
        I: Stream<Item = Event> + Unpin,
        WS: Stream<Item = Result<Message, TungsteniteError>> + Unpin,
        O: Sink<Message, Error = TungsteniteError> + Unpin,
    {
        const PING: &[u8] = b"PING";

        // tokio::time::Interval panics if the period arg is zero. Since the struct members are
        // using NonZeroU64 that is not something we need to account for.
        let mut ping_interval = PingInterval::new(self.ping_interval.map(u64::from));

        if let Err(error) = ws_sink.send(Message::Ping(PING.to_vec())).await {
            emit!(WebSocketConnectionError { error });
            return Err(());
        }
        let mut last_pong = Instant::now();

        let bytes_sent = register!(BytesSent::from(Protocol("websocket".into())));
        let events_sent = register!(EventsSent::from(Output(None)));
        let encode_as_binary = self.should_encode_as_binary();

        loop {
            let result = tokio::select! {
                _ = ping_interval.tick() => {
                    match self.check_received_pong_time(last_pong) {
                        Ok(()) => ws_sink.send(Message::Ping(PING.to_vec())).await.map(|_| ()),
                        Err(e) => Err(e)
                    }
                },

                Some(msg) = ws_stream.next() => {
                    // Pongs are sent automatically by tungstenite during reading from the stream.
                    match msg {
                        Ok(Message::Pong(_)) => {
                            last_pong = Instant::now();
                            Ok(())
                        },
                        Ok(_) => Ok(()),
                        Err(e) => Err(e)
                    }
                },

                event = input.next() => {
                    let mut event = if let Some(event) = event {
                        event
                    } else {
                        break;
                    };

                    let finalizers = event.take_finalizers();

                    self.transformer.transform(&mut event);

                    let event_byte_size = event.estimated_json_encoded_size_of();

                    let mut bytes = BytesMut::new();
                    let res = match self.encoder.encode(event, &mut bytes) {
                        Ok(()) => {
                            finalizers.update_status(EventStatus::Delivered);

                            let message = if encode_as_binary {
                                Message::binary(bytes)
                            }
                            else {
                                Message::text(String::from_utf8_lossy(&bytes))
                            };
                            let message_len = message.len();

                            ws_sink.send(message).await.map(|_| {
                                events_sent.emit(CountByteSize(1, event_byte_size));
                                bytes_sent.emit(ByteSize(message_len));
                            })
                        },
                        Err(_) => {
                            // Error is handled by `Encoder`.
                            finalizers.update_status(EventStatus::Errored);
                            Ok(())
                        }
                    };

                    res
                },
                else => break,
            };

            if let Err(error) = result {
                if is_closed(&error) {
                    emit!(WebSocketConnectionShutdown);
                } else {
                    emit!(WebSocketConnectionError { error });
                }
                return Err(());
            }
        }

        Ok(())
    }
}

#[async_trait]
impl StreamSink<Event> for WebSocketSink {
    async fn run(mut self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let input = input.fuse().peekable();
        pin_mut!(input);

        while input.as_mut().peek().await.is_some() {
            let (ws_sink, ws_stream) = self.create_sink_and_stream().await;
            pin_mut!(ws_sink);
            pin_mut!(ws_stream);

            let _open_token = OpenGauge::new().open(|count| emit!(ConnectionOpen { count }));

            if self
                .handle_events(&mut input, &mut ws_stream, &mut ws_sink)
                .await
                .is_ok()
            {
                _ = ws_sink.close().await;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use futures::{future, FutureExt, StreamExt};
    use serde_json::Value as JsonValue;
    use tokio::{time, time::timeout};
    use tokio_tungstenite::{
        accept_async, accept_hdr_async,
        tungstenite::{
            error::ProtocolError,
            handshake::server::{Request, Response},
        },
    };
    use vector_lib::codecs::JsonSerializerConfig;

    use super::*;
    use crate::{
        common::websocket::WebSocketCommonConfig,
        config::{SinkConfig, SinkContext},
        http::Auth,
        test_util::{
            components::{run_and_assert_sink_compliance, SINK_TAGS},
            next_addr, random_lines_with_stream, trace_init, CountReceiver,
        },
        tls::{self, MaybeTlsSettings, TlsConfig, TlsEnableableConfig},
    };

    #[tokio::test(flavor = "multi_thread")]
    async fn test_websocket() {
        trace_init();

        let addr = next_addr();
        let config = WebSocketSinkConfig {
            common: WebSocketCommonConfig {
                uri: format!("ws://{addr}"),
                tls: None,
                ping_interval: None,
                ping_timeout: None,
                auth: None,
            },
            encoding: JsonSerializerConfig::default().into(),
            acknowledgements: Default::default(),
        };
        let tls = MaybeTlsSettings::Raw(());

        send_events_and_assert(addr, config, tls, None).await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_auth_websocket() {
        trace_init();

        let auth = Some(Auth::Bearer {
            token: "OiJIUzI1NiIsInR5cCI6IkpXVCJ".to_string().into(),
        });
        let auth_clone = auth.clone();
        let addr = next_addr();
        let config = WebSocketSinkConfig {
            common: WebSocketCommonConfig {
                uri: format!("ws://{addr}"),
                tls: None,
                ping_interval: None,
                ping_timeout: None,
                auth: None,
            },
            encoding: JsonSerializerConfig::default().into(),
            acknowledgements: Default::default(),
        };
        let tls = MaybeTlsSettings::Raw(());

        send_events_and_assert(addr, config, tls, auth_clone).await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_tls_websocket() {
        trace_init();

        let addr = next_addr();
        let tls_config = Some(TlsEnableableConfig::test_config());
        let tls = MaybeTlsSettings::from_config(tls_config.as_ref(), true).unwrap();

        let config = WebSocketSinkConfig {
            common: WebSocketCommonConfig {
                uri: format!("wss://{addr}"),
                tls: Some(TlsEnableableConfig {
                    enabled: Some(true),
                    options: TlsConfig {
                        verify_certificate: Some(false),
                        verify_hostname: Some(true),
                        ca_file: Some(tls::TEST_PEM_CRT_PATH.into()),
                        ..Default::default()
                    },
                }),
                ping_timeout: None,
                ping_interval: None,
                auth: None,
            },
            encoding: JsonSerializerConfig::default().into(),
            acknowledgements: Default::default(),
        };

        send_events_and_assert(addr, config, tls, None).await;
    }

    #[tokio::test]
    async fn test_websocket_reconnect() {
        trace_init();

        let addr = next_addr();
        let config = WebSocketSinkConfig {
            common: WebSocketCommonConfig {
                uri: format!("ws://{addr}"),
                tls: None,
                ping_interval: None,
                ping_timeout: None,
                auth: None,
            },
            encoding: JsonSerializerConfig::default().into(),
            acknowledgements: Default::default(),
        };
        let tls = MaybeTlsSettings::Raw(());

        let mut receiver = create_count_receiver(addr, tls.clone(), true, None);

        let context = SinkContext::default();
        let (sink, _healthcheck) = config.build(context).await.unwrap();

        let (_lines, events) = random_lines_with_stream(10, 100, None);
        let events = events.then(|event| async move {
            time::sleep(Duration::from_millis(10)).await;
            event
        });
        drop(tokio::spawn(sink.run(events)));

        receiver.connected().await;
        time::sleep(Duration::from_millis(500)).await;
        assert!(!receiver.await.is_empty());

        let mut receiver = create_count_receiver(addr, tls, false, None);
        assert!(timeout(Duration::from_secs(10), receiver.connected())
            .await
            .is_ok());
    }

    async fn send_events_and_assert(
        addr: SocketAddr,
        config: WebSocketSinkConfig,
        tls: MaybeTlsSettings,
        auth: Option<Auth>,
    ) {
        let mut receiver = create_count_receiver(addr, tls, false, auth);

        let context = SinkContext::default();
        let (sink, _healthcheck) = config.build(context).await.unwrap();

        let (lines, events) = random_lines_with_stream(10, 100, None);
        run_and_assert_sink_compliance(sink, events, &SINK_TAGS).await;

        receiver.connected().await;

        let output = receiver.await;
        assert_eq!(lines.len(), output.len());
        let message_key = crate::config::log_schema()
            .message_key()
            .expect("global log_schema.message_key to be valid path")
            .to_string();
        for (source, received) in lines.iter().zip(output) {
            let json = serde_json::from_str::<JsonValue>(&received).expect("Invalid JSON");
            let received = json.get(message_key.as_str()).unwrap().as_str().unwrap();
            assert_eq!(source, received);
        }
    }

    fn create_count_receiver(
        addr: SocketAddr,
        tls: MaybeTlsSettings,
        interrupt_stream: bool,
        auth: Option<Auth>,
    ) -> CountReceiver<String> {
        CountReceiver::receive_items_stream(move |tripwire, connected| async move {
            let listener = tls.bind(&addr).await.unwrap();
            let stream = listener.accept_stream();

            let tripwire = tripwire.map(|_| ()).shared();
            let stream_tripwire = tripwire.clone();
            let mut connected = Some(connected);

            let stream = stream
                .take_until(tripwire)
                .filter_map(move |maybe_tls_stream| {
                    let au = auth.clone();
                    async move {
                        let maybe_tls_stream = maybe_tls_stream.unwrap();
                        let ws_stream = match au {
                            Some(a) => {
                                let auth_callback = |req: &Request, res: Response| {
                                    let hdr = req.headers().get("Authorization");
                                    if let Some(h) = hdr {
                                        match a {
                                            Auth::Bearer { token } => {
                                                if format!("Bearer {}", token.inner())
                                                    != h.to_str().unwrap()
                                                {
                                                    return Err(
                                                        http::Response::<Option<String>>::new(None),
                                                    );
                                                }
                                            }
                                            Auth::Basic {
                                                user: _user,
                                                password: _password,
                                            } => { /* Not needed for tests at the moment */ }
                                            #[cfg(feature = "aws-core")]
                                            _ => {}
                                        }
                                    }
                                    Ok(res)
                                };
                                accept_hdr_async(maybe_tls_stream, auth_callback)
                                    .await
                                    .unwrap()
                            }
                            None => accept_async(maybe_tls_stream).await.unwrap(),
                        };

                        Some(
                            ws_stream
                                .filter_map(|msg| {
                                    future::ready(match msg {
                                        Ok(msg) if msg.is_text() => {
                                            Some(Ok(msg.into_text().unwrap()))
                                        }
                                        Err(TungsteniteError::Protocol(
                                            ProtocolError::ResetWithoutClosingHandshake,
                                        )) => None,
                                        Err(e) => Some(Err(e)),
                                        _ => None,
                                    })
                                })
                                .take_while(|msg| future::ready(msg.is_ok()))
                                .filter_map(|msg| future::ready(msg.ok())),
                        )
                    }
                })
                .map(move |ws_stream| {
                    connected.take().map(|trigger| trigger.send(()));
                    ws_stream
                })
                .flatten();

            match interrupt_stream {
                false => stream.boxed(),
                true => stream.take_until(stream_tripwire).boxed(),
            }
        })
    }
}
