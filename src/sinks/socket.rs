use vector_lib::codecs::{
    encoding::{Framer, FramingConfig},
    TextSerializerConfig,
};
use vector_lib::configurable::configurable_component;

#[cfg(unix)]
use crate::sinks::util::unix::UnixSinkConfig;
use crate::{
    codecs::{Encoder, EncodingConfig, EncodingConfigWithFraming, SinkType},
    config::{AcknowledgementsConfig, DataType, GenerateConfig, Input, SinkConfig, SinkContext},
    sinks::util::{tcp::TcpSinkConfig, udp::UdpSinkConfig},
};

/// Configuration for the `socket` sink.
#[configurable_component(sink("socket", "Deliver logs to a remote socket endpoint."))]
#[derive(Clone, Debug)]
pub struct SocketSinkConfig {
    #[serde(flatten)]
    pub mode: Mode,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
}

/// Socket mode.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(tag = "mode", rename_all = "snake_case")]
#[configurable(metadata(docs::enum_tag_description = "The type of socket to use."))]
pub enum Mode {
    /// Send over TCP.
    Tcp(TcpMode),

    /// Send over UDP.
    Udp(UdpMode),

    /// Send over a Unix domain socket (UDS).
    #[cfg(unix)]
    Unix(UnixMode),
}

/// TCP configuration.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct TcpMode {
    #[serde(flatten)]
    config: TcpSinkConfig,

    #[serde(flatten)]
    encoding: EncodingConfigWithFraming,
}

/// UDP configuration.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct UdpMode {
    #[serde(flatten)]
    config: UdpSinkConfig,

    #[configurable(derived)]
    encoding: EncodingConfig,
}

/// Unix Domain Socket configuration.
#[cfg(unix)]
#[configurable_component]
#[derive(Clone, Debug)]
pub struct UnixMode {
    #[serde(flatten)]
    config: UnixSinkConfig,

    #[serde(flatten)]
    encoding: EncodingConfigWithFraming,
}

impl GenerateConfig for SocketSinkConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"address = "92.12.333.224:5000"
            mode = "tcp"
            encoding.codec = "json""#,
        )
        .unwrap()
    }
}

impl SocketSinkConfig {
    pub const fn new(mode: Mode, acknowledgements: AcknowledgementsConfig) -> Self {
        SocketSinkConfig {
            mode,
            acknowledgements,
        }
    }

    pub fn make_basic_tcp_config(
        address: String,
        acknowledgements: AcknowledgementsConfig,
    ) -> Self {
        Self::new(
            Mode::Tcp(TcpMode {
                config: TcpSinkConfig::from_address(address),
                encoding: (None::<FramingConfig>, TextSerializerConfig::default()).into(),
            }),
            acknowledgements,
        )
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "socket")]
impl SinkConfig for SocketSinkConfig {
    async fn build(
        &self,
        _cx: SinkContext,
    ) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
        match &self.mode {
            Mode::Tcp(TcpMode { config, encoding }) => {
                let transformer = encoding.transformer();
                let (framer, serializer) = encoding.build(SinkType::StreamBased)?;
                let encoder = Encoder::<Framer>::new(framer, serializer);
                config.build(transformer, encoder)
            }
            Mode::Udp(UdpMode { config, encoding }) => {
                let transformer = encoding.transformer();
                let serializer = encoding.build()?;
                let encoder = Encoder::<()>::new(serializer);
                config.build(transformer, encoder)
            }
            #[cfg(unix)]
            Mode::Unix(UnixMode { config, encoding }) => {
                let transformer = encoding.transformer();
                let (framer, serializer) = encoding.build(SinkType::StreamBased)?;
                let encoder = Encoder::<Framer>::new(framer, serializer);
                config.build(transformer, encoder)
            }
        }
    }

    fn input(&self) -> Input {
        let encoder_input_type = match &self.mode {
            Mode::Tcp(TcpMode { encoding, .. }) => encoding.config().1.input_type(),
            Mode::Udp(UdpMode { encoding, .. }) => encoding.config().input_type(),
            #[cfg(unix)]
            Mode::Unix(UnixMode { encoding, .. }) => encoding.config().1.input_type(),
        };
        Input::new(encoder_input_type & DataType::Log)
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

#[cfg(test)]
mod test {
    use std::{
        future::ready,
        net::{SocketAddr, UdpSocket},
    };

    use futures::stream::StreamExt;
    use futures_util::stream;
    use serde_json::Value;
    use tokio::{
        net::TcpListener,
        time::{sleep, timeout, Duration},
    };
    use tokio_stream::wrappers::TcpListenerStream;
    use tokio_util::codec::{FramedRead, LinesCodec};
    use vector_lib::codecs::JsonSerializerConfig;

    use super::*;
    use crate::{
        config::SinkContext,
        event::{Event, LogEvent},
        test_util::{
            components::{assert_sink_compliance, run_and_assert_sink_compliance, SINK_TAGS},
            next_addr, next_addr_v6, random_lines_with_stream, trace_init, CountReceiver,
        },
    };

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<SocketSinkConfig>();
    }

    async fn test_udp(addr: SocketAddr) {
        let receiver = UdpSocket::bind(addr).unwrap();

        let config = SocketSinkConfig {
            mode: Mode::Udp(UdpMode {
                config: UdpSinkConfig::from_address(addr.to_string()),
                encoding: JsonSerializerConfig::default().into(),
            }),
            acknowledgements: Default::default(),
        };

        let context = SinkContext::default();
        assert_sink_compliance(&SINK_TAGS, async move {
            let (sink, _healthcheck) = config.build(context).await.unwrap();

            let event = Event::Log(LogEvent::from("raw log line"));
            sink.run(stream::once(ready(event.into()))).await
        })
        .await
        .expect("Running sink failed");

        let mut buf = [0; 256];
        let (size, _src_addr) = receiver
            .recv_from(&mut buf)
            .expect("Did not receive message");

        let packet = String::from_utf8(buf[..size].to_vec()).expect("Invalid data received");
        let data = serde_json::from_str::<Value>(&packet).expect("Invalid JSON received");
        let data = data.as_object().expect("Not a JSON object");
        assert!(data.get("timestamp").is_some());
        let message = data.get("message").expect("No message in JSON");
        assert_eq!(message, &Value::String("raw log line".into()));
    }

    #[tokio::test]
    async fn udp_ipv4() {
        trace_init();

        test_udp(next_addr()).await;
    }

    #[tokio::test]
    async fn udp_ipv6() {
        trace_init();

        test_udp(next_addr_v6()).await;
    }

    #[tokio::test]
    async fn tcp_stream() {
        trace_init();

        let addr = next_addr();
        let config = SocketSinkConfig {
            mode: Mode::Tcp(TcpMode {
                config: TcpSinkConfig::from_address(addr.to_string()),
                encoding: (None::<FramingConfig>, JsonSerializerConfig::default()).into(),
            }),
            acknowledgements: Default::default(),
        };

        let mut receiver = CountReceiver::receive_lines(addr);

        let (lines, events) = random_lines_with_stream(10, 100, None);

        assert_sink_compliance(&SINK_TAGS, async move {
            let context = SinkContext::default();
            let (sink, _healthcheck) = config.build(context).await.unwrap();

            sink.run(events).await
        })
        .await
        .expect("Running sink failed");

        // Wait for output to connect
        receiver.connected().await;

        let output = receiver.await;
        assert_eq!(lines.len(), output.len());
        for (source, received) in lines.iter().zip(output) {
            let json = serde_json::from_str::<Value>(&received).expect("Invalid JSON");
            let received = json.get("message").unwrap().as_str().unwrap();
            assert_eq!(source, received);
        }
    }

    // This is a test that checks that we properly receive all events in the
    // case of a proper server side write side shutdown.
    //
    // This test basically sends 10 events, shuts down the server and forces a
    // reconnect. It then forces another 10 events through and we should get a
    // total of 20 events.
    //
    // If this test hangs that means somewhere we are not collecting the correct
    // events.
    #[tokio::test]
    async fn tcp_stream_detects_disconnect() {
        use std::{
            pin::Pin,
            sync::{
                atomic::{AtomicUsize, Ordering},
                Arc,
            },
            task::Poll,
        };

        use futures::{channel::mpsc, FutureExt, SinkExt, StreamExt};
        use tokio::{
            io::{AsyncRead, AsyncWriteExt, ReadBuf},
            net::TcpStream,
            task::yield_now,
            time::{interval, Duration},
        };
        use tokio_stream::wrappers::IntervalStream;

        use crate::event::EventArray;
        use crate::tls::{
            self, MaybeTlsIncomingStream, MaybeTlsSettings, TlsConfig, TlsEnableableConfig,
        };

        trace_init();

        let addr = next_addr();
        let config = SocketSinkConfig {
            mode: Mode::Tcp(TcpMode {
                config: TcpSinkConfig::new(
                    addr.to_string(),
                    None,
                    Some(TlsEnableableConfig {
                        enabled: Some(true),
                        options: TlsConfig {
                            verify_certificate: Some(false),
                            verify_hostname: Some(false),
                            ca_file: Some(tls::TEST_PEM_CRT_PATH.into()),
                            ..Default::default()
                        },
                    }),
                    None,
                ),
                encoding: (None::<FramingConfig>, TextSerializerConfig::default()).into(),
            }),
            acknowledgements: Default::default(),
        };
        let context = SinkContext::default();
        let (sink, _healthcheck) = config.build(context).await.unwrap();
        let (mut sender, receiver) = mpsc::channel::<Option<EventArray>>(0);
        let jh1 = tokio::spawn(async move {
            let stream = receiver
                .take_while(|event| ready(event.is_some()))
                .map(|event| event.unwrap())
                .boxed();
            run_and_assert_sink_compliance(sink, stream, &SINK_TAGS).await
        });

        let msg_counter = Arc::new(AtomicUsize::new(0));
        let msg_counter1 = Arc::clone(&msg_counter);
        let conn_counter = Arc::new(AtomicUsize::new(0));
        let conn_counter1 = Arc::clone(&conn_counter);

        let (close_tx, close_rx) = tokio::sync::oneshot::channel::<()>();
        let mut close_rx = Some(close_rx.map(|x| x.unwrap()));

        let config = Some(TlsEnableableConfig::test_config());

        // Only accept two connections.
        let jh2 = tokio::spawn(async move {
            let tls = MaybeTlsSettings::from_config(&config, true).unwrap();
            let listener = tls.bind(&addr).await.unwrap();
            listener
                .accept_stream()
                .take(2)
                .for_each(|connection| {
                    let mut close_rx = close_rx.take();

                    conn_counter1.fetch_add(1, Ordering::SeqCst);
                    let msg_counter1 = Arc::clone(&msg_counter1);

                    let mut stream: MaybeTlsIncomingStream<TcpStream> = connection.unwrap();

                    std::future::poll_fn(move |cx| loop {
                        if let Some(fut) = close_rx.as_mut() {
                            if let Poll::Ready(()) = fut.poll_unpin(cx) {
                                stream
                                    .get_mut()
                                    .unwrap()
                                    .shutdown()
                                    .now_or_never()
                                    .unwrap()
                                    .unwrap();
                                close_rx = None;
                            }
                        }

                        let mut buf = [0u8; 11];
                        let mut buf = ReadBuf::new(&mut buf);
                        return match Pin::new(&mut stream).poll_read(cx, &mut buf) {
                            Poll::Ready(Ok(())) => {
                                if buf.filled().is_empty() {
                                    Poll::Ready(())
                                } else {
                                    msg_counter1.fetch_add(1, Ordering::SeqCst);
                                    continue;
                                }
                            }
                            Poll::Ready(Err(error)) => panic!("{}", error),
                            Poll::Pending => Poll::Pending,
                        };
                    })
                })
                .await;
        });

        let (_, mut events) = random_lines_with_stream(10, 10, None);
        while let Some(event) = events.next().await {
            sender.send(Some(event)).await.unwrap();
        }

        // Loop and check for 10 events, we should always get 10 events. Once,
        // we have 10 events we can tell the server to shutdown to simulate the
        // remote shutting down on an idle connection.
        IntervalStream::new(interval(Duration::from_millis(100)))
            .take(500)
            .take_while(|_| ready(msg_counter.load(Ordering::SeqCst) != 10))
            .for_each(|_| ready(()))
            .await;
        close_tx.send(()).unwrap();

        // Close connection in spawned future
        yield_now().await;

        assert_eq!(msg_counter.load(Ordering::SeqCst), 10);
        assert_eq!(conn_counter.load(Ordering::SeqCst), 1);

        // Send another 10 events
        let (_, mut events) = random_lines_with_stream(10, 10, None);
        while let Some(event) = events.next().await {
            sender.send(Some(event)).await.unwrap();
        }

        // Wait for server task to be complete.
        sender.send(None).await.unwrap();
        jh1.await.unwrap();
        jh2.await.unwrap();

        // Check that there are exactly 20 events.
        assert_eq!(msg_counter.load(Ordering::SeqCst), 20);
        assert_eq!(conn_counter.load(Ordering::SeqCst), 2);
    }

    /// Tests whether socket recovers from a hard disconnect.
    #[tokio::test]
    async fn reconnect() {
        trace_init();

        let addr = next_addr();
        let config = SocketSinkConfig {
            mode: Mode::Tcp(TcpMode {
                config: TcpSinkConfig::from_address(addr.to_string()),
                encoding: (None::<FramingConfig>, TextSerializerConfig::default()).into(),
            }),
            acknowledgements: Default::default(),
        };

        let context = SinkContext::default();
        let (sink, _healthcheck) = config.build(context).await.unwrap();

        let (_, events) = random_lines_with_stream(1000, 10000, None);
        let sink_handle = tokio::spawn(run_and_assert_sink_compliance(sink, events, &SINK_TAGS));

        // First listener
        let mut count = 20usize;
        TcpListenerStream::new(TcpListener::bind(addr).await.unwrap())
            .next()
            .await
            .unwrap()
            .map(|socket| FramedRead::new(socket, LinesCodec::new()))
            .unwrap()
            .map(|x| x.unwrap())
            .take_while(|_| {
                ready(if count > 0 {
                    count -= 1;
                    true
                } else {
                    false
                })
            })
            .collect::<Vec<_>>()
            .await;

        // Disconnect
        if cfg!(windows) {
            // Gives Windows time to release the addr port.
            sleep(Duration::from_secs(1)).await;
        }

        // Second listener
        // If this doesn't succeed then the sink hanged.
        assert!(timeout(
            Duration::from_secs(5),
            CountReceiver::receive_lines(addr).connected()
        )
        .await
        .is_ok());

        sink_handle.await.unwrap();
    }
}
