#[cfg(unix)]
use crate::sinks::util::unix::UnixSinkConfig;
use crate::{
    config::{DataType, SinkConfig, SinkContext, SinkDescription},
    sinks::util::{encoding::EncodingConfig, tcp::TcpSinkConfig, udp::UdpSinkConfig, Encoding},
    tls::TlsConfig,
};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug)]
// TODO: add back when serde-rs/serde#1358 is addressed
// #[serde(deny_unknown_fields)]
pub struct SocketSinkConfig {
    #[serde(flatten)]
    pub mode: Mode,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum Mode {
    Tcp(TcpSinkConfig),
    Udp(UdpSinkConfig),
    #[cfg(unix)]
    Unix(UnixSinkConfig),
}

inventory::submit! {
    SinkDescription::new_without_default::<SocketSinkConfig>("socket")
}

impl SocketSinkConfig {
    pub fn make_tcp_config(
        address: String,
        encoding: EncodingConfig<Encoding>,
        tls: Option<TlsConfig>,
    ) -> Self {
        TcpSinkConfig {
            address,
            encoding,
            tls,
        }
        .into()
    }

    pub fn make_basic_tcp_config(address: String) -> Self {
        TcpSinkConfig::new(address, EncodingConfig::from(Encoding::Text)).into()
    }
}

impl From<TcpSinkConfig> for SocketSinkConfig {
    fn from(config: TcpSinkConfig) -> Self {
        Self {
            mode: Mode::Tcp(config),
        }
    }
}

impl From<UdpSinkConfig> for SocketSinkConfig {
    fn from(config: UdpSinkConfig) -> Self {
        Self {
            mode: Mode::Udp(config),
        }
    }
}

#[typetag::serde(name = "socket")]
impl SinkConfig for SocketSinkConfig {
    fn build(&self, cx: SinkContext) -> crate::Result<(super::RouterSink, super::Healthcheck)> {
        match &self.mode {
            Mode::Tcp(config) => config.build(cx),
            Mode::Udp(config) => config.build(cx),
            #[cfg(unix)]
            Mode::Unix(config) => config.build(cx),
        }
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "socket"
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        config::SinkContext,
        event::Event,
        test_util::{next_addr, random_lines_with_stream, trace_init, CountReceiver},
    };
    use futures::{
        compat::{Future01CompatExt, Sink01CompatExt},
        SinkExt,
    };
    use futures01::Sink;
    use serde_json::Value;
    use std::net::UdpSocket;

    #[tokio::test]
    async fn udp_message() {
        trace_init();

        let addr = next_addr();
        let receiver = UdpSocket::bind(addr).unwrap();

        let config = SocketSinkConfig {
            mode: Mode::Udp(UdpSinkConfig {
                address: addr.to_string(),
                encoding: Encoding::Json.into(),
            }),
        };
        let context = SinkContext::new_test();
        let (sink, _healthcheck) = config.build(context).unwrap();

        let event = Event::from("raw log line");
        let _ = sink.send(event).compat().await.unwrap();

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
    async fn tcp_stream() {
        trace_init();

        let addr = next_addr();
        let config = SocketSinkConfig {
            mode: Mode::Tcp(TcpSinkConfig {
                address: addr.to_string(),
                encoding: Encoding::Json.into(),
                tls: None,
            }),
        };

        let context = SinkContext::new_test();
        let (sink, _healthcheck) = config.build(context).unwrap();

        let mut receiver = CountReceiver::receive_lines(addr);

        let (lines, mut events) = random_lines_with_stream(10, 100);
        let _ = sink.sink_compat().send_all(&mut events).await.unwrap();

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
    #[cfg(all(feature = "sources-tls", feature = "listenfd"))]
    #[tokio::test]
    async fn tcp_stream_detects_disconnect() {
        use crate::tls::{MaybeTlsIncomingStream, MaybeTlsSettings, TlsConfig, TlsOptions};
        use futures::{future, FutureExt, StreamExt};
        use std::{
            net::Shutdown,
            pin::Pin,
            sync::{
                atomic::{AtomicUsize, Ordering},
                Arc,
            },
            task::Poll,
        };
        use tokio::{
            io::AsyncRead,
            net::TcpStream,
            task::yield_now,
            time::{interval, Duration},
        };

        trace_init();

        let addr = next_addr();
        let config = SocketSinkConfig {
            mode: Mode::Tcp(TcpSinkConfig {
                address: addr.to_string(),
                encoding: Encoding::Text.into(),
                tls: Some(TlsConfig {
                    enabled: Some(true),
                    options: TlsOptions {
                        verify_certificate: Some(false),
                        verify_hostname: Some(false),
                        ca_file: Some("tests/data/localhost.crt".into()),
                        ..Default::default()
                    },
                }),
            }),
        };
        let context = SinkContext::new_test();
        let (sink, _healthcheck) = config.build(context).unwrap();
        let mut sink = sink.sink_compat();

        let msg_counter = Arc::new(AtomicUsize::new(0));
        let msg_counter1 = Arc::clone(&msg_counter);
        let conn_counter = Arc::new(AtomicUsize::new(0));
        let conn_counter1 = Arc::clone(&conn_counter);

        let (close_tx, close_rx) = tokio::sync::oneshot::channel::<()>();
        let mut close_rx = Some(close_rx.map(|x| x.unwrap()));

        let config = Some(TlsConfig {
            enabled: Some(true),
            options: TlsOptions {
                crt_file: Some("tests/data/localhost.crt".into()),
                key_file: Some("tests/data/localhost.key".into()),
                ..Default::default()
            },
        });

        // Only accept two connections.
        let jh = tokio::spawn(async move {
            let tls = MaybeTlsSettings::from_config(&config, true).unwrap();
            let mut listener = tls.bind(&addr).await.unwrap();
            listener
                .incoming()
                .take(2)
                .for_each(|connection| {
                    let mut close_rx = close_rx.take();

                    conn_counter1.fetch_add(1, Ordering::SeqCst);
                    let msg_counter1 = Arc::clone(&msg_counter1);

                    let mut stream: MaybeTlsIncomingStream<TcpStream> = connection.unwrap();
                    future::poll_fn(move |cx| loop {
                        if let Some(fut) = close_rx.as_mut() {
                            if let Poll::Ready(()) = fut.poll_unpin(cx) {
                                stream.get_ref().unwrap().shutdown(Shutdown::Write).unwrap();
                                close_rx = None;
                            }
                        }

                        return match Pin::new(&mut stream).poll_read(cx, &mut [0u8; 11]) {
                            Poll::Ready(Ok(n)) => {
                                if n == 0 {
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

        let (_, mut events) = random_lines_with_stream(10, 10);
        let _ = sink.send_all(&mut events).await.unwrap();

        // Loop and check for 10 events, we should always get 10 events. Once,
        // we have 10 events we can tell the server to shutdown to simulate the
        // remote shutting down on an idle connection.
        interval(Duration::from_millis(100))
            .take(500)
            .take_while(|_| future::ready(msg_counter.load(Ordering::SeqCst) != 10))
            .for_each(|_| future::ready(()))
            .await;
        close_tx.send(()).unwrap();

        // Close connection in spawned future
        yield_now().await;

        assert_eq!(msg_counter.load(Ordering::SeqCst), 10);
        assert_eq!(conn_counter.load(Ordering::SeqCst), 1);

        // Send another 10 events
        let (_, events) = random_lines_with_stream(10, 10);
        events.forward(sink).await.unwrap();

        // Wait for server task to be complete.
        let _ = jh.await.unwrap();

        // Check that there are exactly 20 events.
        assert_eq!(msg_counter.load(Ordering::SeqCst), 20);
        assert_eq!(conn_counter.load(Ordering::SeqCst), 2);
    }
}
