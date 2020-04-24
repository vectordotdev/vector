#[cfg(unix)]
use crate::sinks::util::unix::UnixSinkConfig;
use crate::{
    sinks::util::{encoding::EncodingConfig, tcp::TcpSinkConfig, udp::UdpSinkConfig, Encoding},
    tls::TlsConfig,
    topology::config::{DataType, SinkConfig, SinkContext, SinkDescription},
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
        event::Event,
        test_util::{next_addr, random_lines_with_stream, receive, runtime},
        topology::config::SinkContext,
    };
    use futures01::Sink;
    use serde_json::Value;
    use std::net::UdpSocket;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    use tokio::{io::AsyncReadExt, net::TcpListener};

    #[test]
    fn udp_message() {
        let addr = next_addr();
        let receiver = UdpSocket::bind(addr).unwrap();

        let config = SocketSinkConfig {
            mode: Mode::Udp(UdpSinkConfig {
                address: addr.to_string(),
                encoding: Encoding::Json.into(),
            }),
        };
        let mut rt = runtime();
        let context = SinkContext::new_test(rt.executor());
        let (sink, _healthcheck) = config.build(context).unwrap();

        let event = Event::from("raw log line");
        let pump = sink.send(event.clone());
        rt.block_on(pump).unwrap();

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

    #[test]
    fn tcp_stream() {
        let addr = next_addr();
        let config = SocketSinkConfig {
            mode: Mode::Tcp(TcpSinkConfig {
                address: addr.to_string(),
                encoding: Encoding::Json.into(),
                tls: None,
            }),
        };
        let mut rt = runtime();
        let context = SinkContext::new_test(rt.executor());
        let (sink, _healthcheck) = config.build(context).unwrap();

        let receiver = receive(&addr);

        let (lines, events) = random_lines_with_stream(10, 100);
        let pump = sink.send_all(events);
        let _ = rt.block_on(pump).unwrap();

        let output = receiver.wait();
        assert_eq!(output.len(), lines.len());
        for (source, received) in lines.iter().zip(output) {
            let json = serde_json::from_str::<Value>(&received).expect("Invalid JSON");
            let received = json.get("message").unwrap().as_str().unwrap();
            assert_eq!(source, received);
        }
    }

    // This is a test that checks that we properly receieve all events in the
    // case of a proper server side write side shutdown.
    //
    // This test basically sends 10 events shutsdown the server and forces a
    // reconnect. It then forces another 10 events through and we should get a
    // total of 20 events.
    //
    // If this test hangs that means somewhere we are not collecting the correct
    // events.
    #[test]
    fn tcp_stream_detects_disconnect() {
        let addr = next_addr();
        let config = SocketSinkConfig {
            mode: Mode::Tcp(TcpSinkConfig {
                address: addr.to_string(),
                encoding: Encoding::Text.into(),
                // TODO: enable TLS here since this is where
                // ran into the issue before!
                tls: None,
            }),
        };
        let mut rt = runtime();
        let context = SinkContext::new_test(rt.executor());
        let (sink, _healthcheck) = config.build(context).unwrap();

        let close = Arc::new(tokio::sync::Notify::new());
        let counter = Arc::new(AtomicUsize::new(0));

        let close1 = close.clone();
        let counter1 = counter.clone();
        let jh = rt.spawn_handle(async move {
            let mut listener = TcpListener::bind(&addr).await.unwrap();

            // Only accept two connections after the second connection is done
            // we can exit.
            for _ in 0..2 {
                let (mut conn, _) = listener.accept().await.unwrap();

                loop {
                    let mut buf = vec![0u8; 11];

                    tokio::select! {
                        _ = close1.notified() => {
                            conn.shutdown(std::net::Shutdown::Write).unwrap();
                            break;
                        }

                        res = conn.read(&mut buf) => {
                            let n = res.unwrap();

                           if  n == 0 {
                               break;
                           } else {
                               counter1.fetch_add(1, Ordering::Relaxed);
                           }
                        }
                    };
                }
            }
        });

        let (_, events) = random_lines_with_stream(10, 10);
        let pump = sink.send_all(events);
        let (sink, _) = rt.block_on(pump).unwrap();

        // Loop and check for 10 events, we should always get 10 events. Once,
        // we have 10 events we can tell the server to shutdown to simulate the
        // remote shutting down on an idle connection.
        for _ in 0..100 {
            let amnt = counter.load(Ordering::SeqCst);

            if amnt == 10 {
                close.notify();
                break;
            }

            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        // Send another 10 events
        let (_, events) = random_lines_with_stream(10, 10);
        let pump = sink.send_all(events);
        let pump = rt.block_on(pump).unwrap();

        // Drop the connection to allow the server to fully read from the buffer
        // and exit.
        drop(pump);

        // Wait for server task to be complete.
        rt.block_on_std(jh).unwrap();

        // Check that there are exacty 20 events.
        assert_eq!(counter.load(Ordering::Relaxed), 20);
    }
}
