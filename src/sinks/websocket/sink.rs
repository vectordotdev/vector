use std::{
    num::NonZeroU64,
    time::{Duration, Instant},
};

use crate::{
    codecs::{Encoder, Transformer},
    common::websocket::{PingInterval, WebSocketConnector},
    event::{Event, EventStatus, Finalizable},
    internal_events::{
        ConnectionOpen, OpenGauge, WebSocketConnectionError, WebSocketConnectionShutdown,
    },
    sinks::{util::StreamSink, websocket::config::WebSocketSinkConfig},
};
use async_trait::async_trait;
use bytes::BytesMut;
use futures::{Sink, Stream, StreamExt, pin_mut, sink::SinkExt, stream::BoxStream};
use tokio_util::codec::Encoder as _;
use vector_lib::{
    EstimatedJsonEncodedSizeOf, emit,
    internal_event::{
        ByteSize, BytesSent, CountByteSize, EventsSent, InternalEventHandle as _, Output, Protocol,
    },
};
use yawc::{Frame, OpCode, WebSocketError};

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
        impl Sink<Frame, Error = WebSocketError> + use<>,
        impl Stream<Item = Frame> + use<>,
    ) {
        let ws_stream = self.connector.connect_backoff().await;
        ws_stream.split()
    }

    fn check_received_pong_time(&self, last_pong: Instant) -> Result<(), WebSocketError> {
        if let Some(ping_timeout) = self.ping_timeout
            && last_pong.elapsed() > Duration::from_secs(ping_timeout.into())
        {
            return Err(WebSocketError::ConnectionClosed);
        }

        Ok(())
    }

    async fn handle_events<I, WS, O>(
        &mut self,
        input: &mut I,
        ws_stream: &mut WS,
        ws_sink: &mut O,
    ) -> Result<(), ()>
    where
        I: Stream<Item = Event> + Unpin,
        WS: Stream<Item = Frame> + Unpin,
        O: Sink<Frame, Error = WebSocketError> + Unpin,
    {
        const PING: &[u8] = b"PING";

        // tokio::time::Interval panics if the period arg is zero. Since the struct members are
        // using NonZeroU64 that is not something we need to account for.
        let mut ping_interval = PingInterval::new(self.ping_interval.map(u64::from));

        if let Err(error) = ws_sink.send(Frame::ping(PING)).await {
            emit!(WebSocketConnectionError { error });
            return Err(());
        }
        let mut last_pong = Instant::now();

        let bytes_sent = register!(BytesSent::from(Protocol("websocket".into())));
        let events_sent = register!(EventsSent::from(Output(None)));
        let encode_as_binary = self.encoder.serializer().is_binary();

        loop {
            let result: Result<(), WebSocketError> = tokio::select! {
                _ = ping_interval.tick() => {
                    match self.check_received_pong_time(last_pong) {
                        Ok(()) => ws_sink.send(Frame::ping(PING)).await.map(|_| ()),
                        Err(e) => Err(e)
                    }
                },

                maybe_frame = ws_stream.next() => {
                    match maybe_frame {
                        Some(frame) if frame.opcode() == OpCode::Pong => {
                            // Pongs are sent automatically by yawc during reading from the stream.
                            last_pong = Instant::now();
                            Ok(())
                        },
                        Some(frame) if frame.opcode() == OpCode::Close => {
                            // Remote closed the connection
                            Err(WebSocketError::ConnectionClosed)
                        },
                        Some(_) => Ok(()),
                        None => {
                            // Stream ended — connection lost
                            Err(WebSocketError::ConnectionClosed)
                        }
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
                    match self.encoder.encode(event, &mut bytes) {
                        Ok(()) => {
                            finalizers.update_status(EventStatus::Delivered);

                            let frame = if encode_as_binary {
                                Frame::binary(bytes.freeze())
                            }
                            else {
                                Frame::text(String::from_utf8_lossy(&bytes).into_owned())
                            };
                            let frame_len = frame.payload().len();

                            ws_sink.send(frame).await.map(|_| {
                                events_sent.emit(CountByteSize(1, event_byte_size));
                                bytes_sent.emit(ByteSize(frame_len));
                            })
                        },
                        Err(_) => {
                            // Error is handled by `Encoder`.
                            finalizers.update_status(EventStatus::Errored);
                            Ok(())
                        }
                    }
                },
                else => break,
            };

            if let Err(error) = result {
                if error.is_closed() {
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

    use bytes::Bytes;
    use futures::{FutureExt, StreamExt};
    use http_body_util::Empty;
    use hyper1::{body::Incoming, service::service_fn};
    use hyper_util::rt::TokioIo;
    use serde_json::Value as JsonValue;
    use tokio::{time, time::timeout};
    use vector_lib::codecs::JsonSerializerConfig;
    use yawc::{OpCode, WebSocket as YawcWebSocket};

    use super::*;
    use crate::{
        common::websocket::WebSocketCommonConfig,
        config::{SinkConfig, SinkContext},
        http::Auth,
        test_util::{
            CountReceiver,
            addr::next_addr,
            components::{SINK_TAGS, run_and_assert_sink_compliance},
            random_lines_with_stream, trace_init,
        },
        tls::{self, MaybeTlsSettings, TlsConfig, TlsEnableableConfig},
    };

    #[tokio::test(flavor = "multi_thread")]
    async fn test_websocket() {
        trace_init();

        let (_guard, addr) = next_addr();
        let config = WebSocketSinkConfig {
            common: WebSocketCommonConfig {
                uri: format!("ws://{addr}"),
                tls: None,
                ping_interval: None,
                ping_timeout: None,
                auth: None,
                compression: None,
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
        let (_guard, addr) = next_addr();
        let config = WebSocketSinkConfig {
            common: WebSocketCommonConfig {
                uri: format!("ws://{addr}"),
                tls: None,
                ping_interval: None,
                ping_timeout: None,
                auth: None,
                compression: None,
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

        let (_guard, addr) = next_addr();
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
                compression: None,
            },
            encoding: JsonSerializerConfig::default().into(),
            acknowledgements: Default::default(),
        };

        send_events_and_assert(addr, config, tls, None).await;
    }

    #[tokio::test]
    async fn test_websocket_reconnect() {
        trace_init();

        let (_guard, addr) = next_addr();
        let config = WebSocketSinkConfig {
            common: WebSocketCommonConfig {
                uri: format!("ws://{addr}"),
                tls: None,
                ping_interval: None,
                ping_timeout: None,
                auth: None,
                compression: None,
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
        assert!(
            timeout(Duration::from_secs(10), receiver.connected())
                .await
                .is_ok()
        );
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
                        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<String>();

                        // Use hyper1 to handle the HTTP upgrade, then yawc for WebSocket
                        let io = TokioIo::new(maybe_tls_stream);
                        let au_clone = au.clone();
                        let service = service_fn(move |mut req: hyper1::Request<Incoming>| {
                            let tx = tx.clone();
                            let au = au_clone.clone();
                            async move {
                                // Validate auth if required
                                if let Some(a) = au {
                                    let hdr = req.headers().get("Authorization");
                                    if let Some(h) = hdr {
                                        match a {
                                            Auth::Bearer { token } => {
                                                if format!("Bearer {}", token.inner())
                                                    != h.to_str().unwrap()
                                                {
                                                    return Ok::<_, hyper1::Error>(
                                                        hyper1::Response::builder()
                                                            .status(401)
                                                            .body(Empty::<Bytes>::new())
                                                            .unwrap(),
                                                    );
                                                }
                                            }
                                            Auth::Basic { .. } => {}
                                            Auth::Custom { .. } => {}
                                            #[cfg(feature = "aws-core")]
                                            _ => {}
                                        }
                                    }
                                }

                                let (response, upgrade_fut) =
                                    YawcWebSocket::upgrade(&mut req).expect("upgrade failed");

                                tokio::spawn(async move {
                                    if let Ok(ws) = upgrade_fut.await {
                                        let mut ws_stream =
                                            futures::StreamExt::fuse(ws);
                                        while let Some(frame) =
                                            futures::StreamExt::next(&mut ws_stream).await
                                        {
                                            if frame.opcode() == OpCode::Text {
                                                let text = std::str::from_utf8(
                                                    frame.payload(),
                                                )
                                                .unwrap()
                                                .to_string();
                                                if tx.send(text).is_err() {
                                                    break;
                                                }
                                            } else if frame.opcode() == OpCode::Close {
                                                break;
                                            }
                                        }
                                    }
                                });

                                Ok(response)
                            }
                        });

                        tokio::spawn(async move {
                            let _ = hyper1::server::conn::http1::Builder::new()
                                .serve_connection(io, service)
                                .with_upgrades()
                                .await;
                        });

                        Some(tokio_stream::wrappers::UnboundedReceiverStream::new(rx))
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
